import importlib.util
from pathlib import Path
import tempfile
import unittest


class NoticeTests(unittest.TestCase):
    def test_vendored_library_is_not_omitted_and_null_metadata_is_supported(self):
        path = Path(__file__).with_name('dependency-notices.py')
        spec = importlib.util.spec_from_file_location('dependency_notices', path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / 'LICENSE').write_text('Upstream license fixture')
            packages = [
                {'id': 'app', 'name': 'fluxion-app', 'version': '1', 'source': None, 'metadata': None},
                {'id': 'vendor', 'name': 'librqbit', 'version': '9', 'source': None,
                 'metadata': {'fluxion-vendored': {'upstream': 'https://github.com/ikatson/rqbit', 'changes': 'Peer policy'}},
                 'manifest_path': str(directory / 'Cargo.toml'), 'license': 'Apache-2.0'},
                {'id': 'registry', 'name': 'registry-fixture', 'version': '1', 'source': 'registry', 'metadata': None,
                 'manifest_path': str(directory / 'Cargo.toml'), 'license': 'Apache-2.0'},
            ]
            nodes = [{'id': 'app', 'deps': [{'pkg': name, 'dep_kinds': [{'kind': None}]} for name in ['vendor', 'registry']]},
                     {'id': 'vendor', 'deps': []}, {'id': 'registry', 'deps': []}]
            output = module.render({'packages': packages, 'resolve': {'nodes': nodes}})
            self.assertIn('## librqbit 9', output)
            self.assertIn('Fluxion modifications: Peer policy', output)
            self.assertIn('## registry-fixture 1', output)
            self.assertIn('Upstream license fixture', output)
