#!/usr/bin/env python3
"""Build a native Fluxion.app, optionally a universal binary and DMG.

No JavaScript build tools or web runtime are involved. Signing is optional for
local builds; release CI requires a Developer ID identity and notarization.
"""
import argparse
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import json
import re
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def validate_release(version, channel):
    if channel not in ('stable', 'beta'):
        raise ValueError('Unknown release channel')
    number = r'(?:0|[1-9][0-9]*)'
    if not re.fullmatch(rf'{number}\.{number}\.{number}(?:-beta\.{number})?', version):
        raise ValueError('Version must be SemVer or SemVer-beta.N')
    if (channel == 'beta') != ('-beta.' in version):
        raise ValueError('Release channel does not match version')


def notarize(path):
    # The private key is passed as a protected file, never as a command argument.
    with tempfile.TemporaryDirectory(prefix='fluxion-notary-') as directory:
        key = Path(directory) / 'AuthKey.p8'
        key.write_text(os.environ['APPLE_API_KEY'])
        key.chmod(0o600)
        upload = path
        if path.suffix == '.app':
            upload = Path(directory) / 'Fluxion.zip'
            run('ditto', '-c', '-k', '--keepParent', str(path), str(upload))
        result = subprocess.run([
            'xcrun', 'notarytool', 'submit', str(upload), '--key', str(key),
            '--key-id', os.environ['APPLE_API_KEY_ID'], '--issuer', os.environ['APPLE_API_ISSUER'],
            '--wait', '--output-format', 'json',
        ], capture_output=True, text=True, timeout=1200)
        if result.returncode or json.loads(result.stdout).get('status') != 'Accepted':
            raise RuntimeError('Apple notarization failed; release artifacts were not published')
    run('xcrun', 'stapler', 'staple', str(path))
    run('xcrun', 'stapler', 'validate', str(path))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--release', action='store_true')
    parser.add_argument('--universal', action='store_true')
    parser.add_argument('--dmg', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--preview', action='store_true', help='Use a separate app identity and disposable data directory')
    parser.add_argument('--notarize', action='store_true', help='Require Developer ID signing and Apple notarization')
    parser.add_argument('--version', default='0.1.0')
    parser.add_argument('--channel', choices=['stable', 'beta'], default='stable')
    parser.add_argument('--distribution', choices=['direct', 'app-store'], default='direct')
    parser.add_argument('--build-number', default='1', help='Numeric Mac App Store build number')
    parser.add_argument('--provisioning-profile', type=Path)
    args = parser.parse_args()
    validate_release(args.version, args.channel)
    store = args.distribution == 'app-store'
    if store:
        if args.dmg or args.notarize or args.channel != 'stable' or args.preview or args.skip_build:
            parser.error('App Store builds require a fresh stable build without DMG, notarization or preview identity')
        if not re.fullmatch(r'[1-9][0-9]*', args.build_number):
            parser.error('--build-number must be a positive integer')
    identity = os.environ.get('APPLE_SIGNING_IDENTITY') or '-'
    if store and identity != '-' and not identity.startswith(('Apple Distribution:', '3rd Party Mac Developer Application:')):
        parser.error('App Store signing requires an Apple Distribution or Mac App Distribution identity')
    if store and identity != '-' and not args.provisioning_profile:
        parser.error('Signed App Store builds require --provisioning-profile')
    identity_settings = {}
    if args.provisioning_profile:
        from release.store_profile import read_identity_entitlements
        if not store or identity == '-':
            parser.error('A provisioning profile is used only with a distribution-signed App Store build')
        try:
            identity_settings = read_identity_entitlements(args.provisioning_profile, 'top.backrunner.fluxion')
        except (ValueError, KeyError, OSError, subprocess.CalledProcessError):
            parser.error('Provisioning profile must be valid, unexpired and match the App Store bundle')
    if args.notarize:
        if not args.release or args.preview or not identity.startswith('Developer ID Application:'):
            parser.error('--notarize requires a release build and Developer ID Application identity')
        for key in ['APPLE_API_KEY', 'APPLE_API_KEY_ID', 'APPLE_API_ISSUER']:
            if not os.environ.get(key):
                parser.error(f'{key} is required for notarization')
    profile = 'release' if args.release else 'debug'
    env = dict(os.environ, FLUXION_VERSION=args.version, FLUXION_CHANNEL=args.channel)
    targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin'] if args.universal else [None]
    binaries = []
    browser_hosts = []
    for target in targets:
        cargo = ['cargo', 'build', '--locked', '-p', 'fluxion-app']
        if store:
            cargo += ['--features', 'fluxion-app/app-store']
        else:
            cargo += ['-p', 'fluxion-browser']
        if args.release:
            cargo.append('--release')
        if target:
            cargo += ['--target', target]
        if not args.skip_build:
            run(*cargo, env=env)
        binaries.append(ROOT / 'target' / (target or '') / profile / 'fluxion-app')
        if not store:
            browser_hosts.append(ROOT / 'target' / (target or '') / profile / 'fluxion-browser-host')
    output = ROOT / 'target' / ('universal-apple-darwin' if args.universal else 'native') / profile / ('app-store' if store else 'bundle')
    app = output / ('Fluxion Preview.app' if args.preview else 'Fluxion.app')
    if app.exists():
        shutil.rmtree(app)
    macos = app / 'Contents/MacOS'
    resources = app / 'Contents/Resources'
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)
    executable = macos / 'fluxion-app'
    if args.universal:
        run('lipo', '-create', *(str(p) for p in binaries), '-output', str(executable))
    else:
        shutil.copy2(binaries[0], executable)
    browser_host = macos / 'fluxion-browser-host'
    if not store:
        if args.universal:
            run('lipo', '-create', *(str(p) for p in browser_hosts), '-output', str(browser_host))
        else:
            shutil.copy2(browser_hosts[0], browser_host)
        shutil.copytree(ROOT / 'browser-extension', resources / 'browser-extension',
                    ignore=shutil.ignore_patterns('*.test.mjs'))
    shutil.copy2(ROOT / 'desktop/assets/app/icon.icns', resources / 'icon.icns')
    plist = {
        'CFBundleName': 'Fluxion', 'CFBundleDisplayName': 'Fluxion',
        'CFBundleIdentifier': 'top.backrunner.fluxion',
        'CFBundleExecutable': 'fluxion-app', 'CFBundleIconFile': 'icon.icns',
        'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': args.version,
        'CFBundleVersion': args.build_number if store else args.version, 'LSMinimumSystemVersion': '11.0',
        'FluxionReleaseChannel': args.channel,
        'NSHighResolutionCapable': True, 'NSPrincipalClass': 'NSApplication',
        'LSApplicationCategoryType': 'public.app-category.utilities',
    }
    if store:
        plist['FluxionDistribution'] = 'app-store'
        plist.pop('FluxionReleaseChannel')
        shutil.copy2(ROOT / 'desktop/app-store/PrivacyInfo.xcprivacy', resources / 'PrivacyInfo.xcprivacy')
        if args.provisioning_profile:
            shutil.copy2(args.provisioning_profile, app / 'Contents/embedded.provisionprofile')
    shutil.copy2(ROOT / 'LICENSE', resources / 'LICENSE.txt')
    shutil.copy2(ROOT / 'docs/app-store/privacy.md', resources / 'Privacy.md')
    run('python3', 'scripts/release/dependency-notices.py', '--output', str(resources / 'ThirdPartyNotices.md'))
    if args.preview:
        plist.update(CFBundleName='Fluxion Preview', CFBundleDisplayName='Fluxion Preview',
                     CFBundleIdentifier='top.backrunner.fluxion.gpui-preview',
                     LSEnvironment={'FLUXION_DATA_DIR': '/tmp/fluxion-gpui-ui-check'})
    (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(plist))
    command = ['codesign', '--force', '--sign', identity]
    if identity != '-':
        command += ['--options', 'runtime', '--timestamp']
    if not store:
        run(*command, str(browser_host))
    if store:
        entitlements = plistlib.loads((ROOT / 'desktop/app-store/Fluxion.entitlements').read_bytes())
        entitlements.update(identity_settings)
        signing_entitlements = output / 'Fluxion.entitlements'
        signing_entitlements.write_bytes(plistlib.dumps(entitlements))
        command += ['--entitlements', str(signing_entitlements)]
    run(*command, str(app))
    run('codesign', '--verify', '--deep', '--strict', str(app))
    if args.notarize:
        notarize(app)
        run('spctl', '--assess', '--type', 'execute', '--verbose', str(app))
    if args.release and not store:
        archive = output / 'Fluxion.app.tar.gz'
        # System tar includes metadata entries. Python's tar emits only ordinary files/directories.
        import tarfile
        with tarfile.open(archive, 'w:gz', format=tarfile.USTAR_FORMAT) as bundle:
            bundle.add(app, arcname='Fluxion.app')
    if args.dmg:
        stage = output / 'dmg-root'
        if stage.exists():
            shutil.rmtree(stage)
        stage.mkdir()
        shutil.copytree(app, stage / 'Fluxion.app')
        (stage / 'Applications').symlink_to('/Applications')
        dmg = output / f'Fluxion_{args.version}_universal.dmg'
        if dmg.exists():
            dmg.unlink()
        run('hdiutil', 'create', '-volname', 'Fluxion', '-srcfolder', str(stage), '-ov', '-format', 'UDZO', str(dmg))
        if args.notarize:
            run('codesign', '--force', '--sign', identity, '--timestamp', str(dmg))
            notarize(dmg)
        shutil.rmtree(stage)
    print(app)


if __name__ == '__main__':
    main()
