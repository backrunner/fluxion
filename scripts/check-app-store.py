#!/usr/bin/env python3
"""Verify the store bundle's technical policy; does not establish Apple approval."""
import argparse
from pathlib import Path
import plistlib
import re
import subprocess
from release.store_profile import read_identity_entitlements


def inspect(app, submission=False):
    contents = app / 'Contents'
    info = plistlib.loads((contents / 'Info.plist').read_bytes())
    assert info.get('FluxionDistribution') == 'app-store', 'Not an App Store bundle'
    assert re.fullmatch(r'\d+\.\d+\.\d+', info['CFBundleShortVersionString']), 'Store version must be numeric'
    assert re.fullmatch(r'[1-9][0-9]*', info['CFBundleVersion']), 'Build number must be a positive integer'
    assert 'FluxionReleaseChannel' not in info, 'Store bundles must not use an updater channel'
    resources = contents / 'Resources'
    for name in ['PrivacyInfo.xcprivacy', 'Privacy.md', 'LICENSE.txt', 'ThirdPartyNotices.md']:
        assert (resources / name).is_file(), f'Missing {name}'
    privacy = plistlib.loads((resources / 'PrivacyInfo.xcprivacy').read_bytes())
    assert privacy['NSPrivacyTracking'] is False, 'Unexpected tracking declaration'
    assert not (contents / 'MacOS/fluxion-browser-host').exists(), 'External browser host is included'
    assert not (resources / 'browser-extension').exists(), 'External browser extension is included'
    executable = contents / 'MacOS' / info['CFBundleExecutable']
    binary = executable.read_bytes()
    for prohibited in [b'https://assets.fluxion.alkinum.io', b'max_wait=90', b'fluxion-update-test']:
        assert prohibited not in binary, 'Direct updater code is present'
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    result = subprocess.run(['codesign', '-d', '--entitlements', ':-', str(app)], capture_output=True, check=True)
    entitlements = plistlib.loads(result.stdout)
    for key in ['app-sandbox', 'network.client', 'network.server', 'files.downloads.read-write',
                'files.user-selected.read-write', 'files.bookmarks.app-scope']:
        assert entitlements.get('com.apple.security.' + key) is True, f'Missing entitlement {key}'
    assert not entitlements.get('com.apple.security.get-task-allow'), 'Debugging entitlement is enabled'
    assert not any('temporary-exception' in k for k in entitlements), 'Temporary sandbox exception is present'
    if submission:
        assert (contents / 'embedded.provisionprofile').is_file(), 'Missing provisioning profile'
        expected = read_identity_entitlements(contents / 'embedded.provisionprofile', info['CFBundleIdentifier'])
        for key, value in expected.items():
            assert entitlements.get(key) == value, f'Signature/profile mismatch: {key}'
        result = subprocess.run(['codesign', '-dv', str(app)], capture_output=True, text=True, check=True)
        assert any(label in result.stderr for label in ['Authority=Apple Distribution:', 'Authority=3rd Party Mac Developer Application:']), 'Not signed for App Store distribution'
        assert 'TeamIdentifier=' + expected['com.apple.developer.team-identifier'] in result.stderr, 'Signing team does not match the profile'
        subprocess.run(['lipo', '-verify_arch', 'arm64', 'x86_64', str(executable)], check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('app', type=Path)
    parser.add_argument('--submission', action='store_true', help='Also require distribution signing and a universal executable')
    args = parser.parse_args()
    try:
        inspect(args.app, args.submission)
    except (AssertionError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'App Store bundle check failed: {error}\n')
    print('App Store technical bundle checks passed. Publisher declarations and Apple validation are separate gates.')


if __name__ == '__main__':
    main()
