import contextlib
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from scripts.bootstrap_components import bootstrap


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.origin = self.base / 'origin'
        self.run_git(['init', str(self.origin)])
        self.run_git(['-C', str(self.origin), 'config', 'user.name', 'Test'])
        self.run_git(['-C', str(self.origin), 'config', 'user.email', 'test@example.invalid'])
        (self.origin / 'hello.txt').write_text('first')
        self.run_git(['-C', str(self.origin), 'add', '.'])
        self.run_git(['-C', str(self.origin), 'commit', '-m', 'first'])
        self.commit = self.run_git(['-C', str(self.origin), 'rev-parse', 'HEAD']).stdout.strip()
        (self.origin / 'hello.txt').write_text('second')
        self.run_git(['-C', str(self.origin), 'commit', '-am', 'second'])
        self.root = self.base / 'siblings'
        self.pin = {'name': 'component', 'commit': self.commit}
        self.real_run = subprocess.run

    def run_git(self, args):
        return subprocess.run(['git', *args], check=True, capture_output=True, text=True)

    def local_run(self, args, **kwargs):
        args = list(args)
        if args[-1] == 'https://github.com/c1cad4/component.git':
            args[-1] = str(self.origin)
        kwargs.setdefault('text', True)
        if 'stdout' not in kwargs and 'stderr' not in kwargs:
            kwargs['capture_output'] = True
        return self.real_run(args, **kwargs)

    def boot(self, rows=None, verify=False):
        with patch('scripts.bootstrap_components.subprocess.run', side_effect=self.local_run):
            with contextlib.redirect_stdout(io.StringIO()):
                bootstrap(self.root, rows if rows is not None else [self.pin], verify)

    def test_fetches_exact_old_commit_and_verifies_without_network(self):
        self.boot()
        target = self.root / 'component'
        self.assertEqual((target / 'hello.txt').read_text(), 'first')
        self.assertEqual(self.run_git(['-C', str(target), 'rev-parse', 'HEAD']).stdout.strip(), self.commit)
        self.assertEqual(self.run_git(['-C', str(target), 'rev-list', '--count', 'HEAD']).stdout.strip(), '1')
        with patch('scripts.bootstrap_components.subprocess.run', side_effect=self.local_run) as run:
            with contextlib.redirect_stdout(io.StringIO()):
                bootstrap(self.root, [self.pin], verify=True)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][-2:], ['rev-parse', 'HEAD'])

    def test_fetch_failure_leaves_no_target_and_can_retry(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.boot([{'name': 'component', 'commit': '0' * 40}])
        self.assertEqual(list(self.root.iterdir()), [])
        self.boot()
        self.assertTrue((self.root / 'component' / 'hello.txt').exists())

    def test_existing_changes_are_preserved_even_on_verify_mismatch(self):
        self.boot()
        file = self.root / 'component' / 'hello.txt'
        file.write_text('local edits')
        self.boot()
        with self.assertRaisesRegex(ValueError, 'differs from lock'):
            self.boot([{'name': 'component', 'commit': '0' * 40}], verify=True)
        self.assertEqual(file.read_text(), 'local edits')

    def test_entire_lock_is_validated_before_creating_root(self):
        for bad in ('../escape', '--option', '.', '..', '', None):
            with self.subTest(name=bad), self.assertRaises(ValueError):
                self.boot([self.pin, {'name': bad, 'commit': self.commit}])
            self.assertFalse(self.root.exists())

    def test_case_insensitive_duplicate_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            self.boot([self.pin, {'name': 'COMPONENT', 'commit': self.commit}])
        self.assertFalse(self.root.exists())

    def test_verify_missing_root_does_not_create_it(self):
        with self.assertRaisesRegex(ValueError, 'missing'):
            self.boot(verify=True)
        self.assertFalse(self.root.exists())

    def test_symlink_is_rejected_without_touching_destination(self):
        self.root.mkdir()
        (self.root / 'component').symlink_to(self.origin, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symbolic link'):
            self.boot()
        self.assertEqual((self.origin / 'hello.txt').read_text(), 'second')

    def test_non_git_directory_is_preserved(self):
        target = self.root / 'component'
        target.mkdir(parents=True)
        (target / 'personal.txt').write_text('keep')
        with self.assertRaisesRegex(ValueError, 'not a Git checkout'):
            self.boot()
        self.assertEqual((target / 'personal.txt').read_text(), 'keep')


if __name__ == '__main__':
    unittest.main()
