import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='fluxion updater test ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.installed = self.root / 'Applications' / 'Fluxion.app'
        self.staged = self.root / '.fluxion-update-test' / 'Fluxion.app'
        self.backup = self.staged.parent / 'previous.app'
        for app, contents in [(self.installed, 'old'), (self.staged, 'new')]:
            app.mkdir(parents=True)
            (app / 'version').write_text(contents)
        self.env = dict(os.environ, MOCK_STAGED=str(self.staged))
        opener = self.root / 'mock open'
        opener.write_text('''#!/usr/bin/env python3
import os, pathlib, sys
app = pathlib.Path(sys.argv[2])
sys.exit(1 if os.environ.get('MOCK_OPEN_FAIL') and (app / 'version').read_text() == 'new' else 0)
''')
        mover = self.root / 'mock mv'
        mover.write_text('''#!/usr/bin/env python3
import os, sys
if os.environ.get('MOCK_MOVE_FAIL') and sys.argv[1] == os.environ['MOCK_STAGED']:
    sys.exit(1)
os.execv('/bin/mv', ['/bin/mv', *sys.argv[1:]])
''')
        opener.chmod(0o700)
        mover.chmod(0o700)
        self.script = (ROOT / 'desktop/native/install-update.sh').read_text().replace(
            '/usr/bin/open', shlex.quote(str(opener))).replace('/bin/mv', shlex.quote(str(mover)))

    def install(self, pid=99999999):
        script = self.root / 'install.sh'
        script.write_text(self.script)
        return subprocess.run(['/bin/sh', str(script), str(self.installed), str(self.staged),
                               str(self.backup), str(pid)], env=self.env, capture_output=True, timeout=5)

    def test_replaces_after_exit_and_retains_recovery_bundle(self):
        self.assertEqual(self.install().returncode, 0)
        self.assertEqual((self.installed / 'version').read_text(), 'new')
        self.assertEqual((self.backup / 'version').read_text(), 'old')

    def test_restores_original_if_second_rename_fails(self):
        self.env['MOCK_MOVE_FAIL'] = '1'
        self.assertNotEqual(self.install().returncode, 0)
        self.assertEqual((self.installed / 'version').read_text(), 'old')

    def test_restores_original_if_launch_command_fails(self):
        self.env['MOCK_OPEN_FAIL'] = '1'
        self.assertNotEqual(self.install().returncode, 0)
        self.assertEqual((self.installed / 'version').read_text(), 'old')
        self.assertEqual((Path(str(self.staged) + '.failed') / 'version').read_text(), 'new')

    def test_never_replaces_a_running_app_after_wait_timeout(self):
        self.script = self.script.replace('max_wait=90', 'max_wait=0')
        self.assertNotEqual(self.install(os.getpid()).returncode, 0)
        self.assertEqual((self.installed / 'version').read_text(), 'old')
        self.assertFalse(self.backup.exists())


class BundleTests(unittest.TestCase):
    def test_rejects_invalid_build_channel_and_version_before_building(self):
        spec = importlib.util.spec_from_file_location('bundle', ROOT / 'scripts/bundle-macos.py')
        bundle = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bundle)
        bundle.validate_release('1.2.3', 'stable')
        bundle.validate_release('1.2.3-beta.10', 'beta')
        for version, channel in [('1.0.0-beta.1', 'stable'), ('1.0.0', 'beta'), ('01.2.3', 'stable')]:
            with self.assertRaises(ValueError):
                bundle.validate_release(version, channel)


if __name__ == '__main__':
    unittest.main()
