from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "package_friends_alpha.py"
SPEC = importlib.util.spec_from_file_location("package_friends_alpha", SCRIPT)
assert SPEC and SPEC.loader
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


class FriendsAlphaPackageTests(unittest.TestCase):
    def make_source(self, root: Path) -> Path:
        source = root / "source"
        for name in package.STATIC_FILES:
            path = source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"project-owned {name}\n", encoding="utf-8")
        release = source / "target" / "release"
        release.mkdir(parents=True)
        for name in package.BINARIES:
            (release / name).write_bytes(b"project executable")
        classes = source / "assets" / "classes"
        classes.mkdir(parents=True)
        (classes / "default.txt").write_text("profile Default\n", encoding="utf-8")
        return source

    def test_package_is_allowlisted_clean_and_verifiable(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = self.make_source(root)
            output = root / "out"
            package.create_package(source, output, build=False)
            package.check_package(output)
            self.assertTrue((output / "vcod.exe").is_file())
            self.assertTrue((output / "assets/classes/default.txt").is_file())
            self.assertFalse((output / "target").exists())

    def test_check_rejects_a_prohibited_retail_extension(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = self.make_source(root)
            output = root / "out"
            package.create_package(source, output, build=False)
            (output / "retail.ff").write_bytes(b"not allowed")
            with self.assertRaisesRegex(RuntimeError, "prohibited"):
                package.check_package(output)

    def test_clean_refuses_an_unmarked_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "out"
            output.mkdir()
            (output / "keep.txt").write_text("do not delete", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "unmarked"):
                package.clean_output(output)
            self.assertTrue((output / "keep.txt").is_file())

    def test_check_rejects_tampering_and_unlisted_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = self.make_source(root)
            output = root / "out"
            package.create_package(source, output, build=False)
            (output / "README.md").write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "SHA-256"):
                package.check_package(output)
            (output / "README.md").write_text("project-owned README.md\n", encoding="utf-8")
            (output / "extra.txt").write_text("extra", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "extra"):
                package.check_package(output)


if __name__ == "__main__":
    unittest.main()
