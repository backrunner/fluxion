import subprocess
import unittest
from pathlib import Path
from datetime import datetime, timezone, timedelta
import importlib.util

ROOT = Path(__file__).resolve().parents[2]


class StoreProfileTests(unittest.TestCase):
    def test_identity_is_scoped_to_bundle_and_rejects_wrong_or_expired_profiles(self):
        spec = importlib.util.spec_from_file_location('store_profile', ROOT / 'scripts/release/store_profile.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        now = datetime.now(timezone.utc)
        profile = {
            'ExpirationDate': now + timedelta(days=1), 'TeamIdentifier': ['TEAM'],
            'ApplicationIdentifierPrefix': ['PREFIX'],
            'Entitlements': {'com.apple.application-identifier': 'PREFIX.top.backrunner.fluxion',
                             'com.apple.developer.team-identifier': 'TEAM',
                             'keychain-access-groups': ['PREFIX.*']},
        }
        result = module.identity_entitlements(profile, 'top.backrunner.fluxion', now)
        self.assertEqual(result['keychain-access-groups'], ['PREFIX.top.backrunner.fluxion'])
        self.assertEqual(result['com.apple.developer.team-identifier'], 'TEAM')
        with self.assertRaises(ValueError):
            module.identity_entitlements(profile, 'another.app', now)
        with self.assertRaises(ValueError):
            module.identity_entitlements(profile, 'top.backrunner.fluxion', now + timedelta(days=2))
        profile['Entitlements']['get-task-allow'] = True
        with self.assertRaises(ValueError):
            module.identity_entitlements(profile, 'top.backrunner.fluxion', now)


class StorePackagingTests(unittest.TestCase):
    def test_invalid_store_options_fail_before_building(self):
        for options in [
            ['--dmg'], ['--notarize'], ['--preview'], ['--skip-build'],
            ['--channel', 'beta', '--version', '1.0.0-beta.1'],
            ['--build-number', '1.0'], ['--build-number', '0'],
        ]:
            result = subprocess.run(['python3', str(ROOT / 'scripts/bundle-macos.py'),
                                     '--distribution', 'app-store', *options], capture_output=True)
            self.assertEqual(result.returncode, 2, options)
            self.assertIn(b'error:', result.stderr)


if __name__ == '__main__':
    unittest.main()
