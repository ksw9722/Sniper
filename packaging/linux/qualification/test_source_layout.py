"""Pure inert-text source-layout tests; no APT or image execution."""
from pathlib import Path
import subprocess
import unittest

BASE = """Types: deb
# Official snapshot comment is informational.
URIs: http://deb.debian.org/debian
Suites: trixie trixie-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp

Types: deb
URIs: http://deb.debian.org/debian-security
Suites: trixie-security
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp
"""


class SourceLayoutTests(unittest.TestCase):
    def parse(self, text):
        return subprocess.run(["awk", "-f", str(Path(__file__).with_name("check_debian_sources.awk"))], input=text, text=True, capture_output=True, check=False)

    def test_both_official_key_filenames_and_protocols(self):
        for extension in ("pgp", "gpg"):
            for protocol in ("http", "https"):
                with self.subTest(extension=extension, protocol=protocol):
                    result = self.parse(BASE.replace(".pgp", "." + extension).replace("http:", protocol + ":"))
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.splitlines(), ["/usr/share/keyrings/debian-archive-keyring." + extension] * 2)

    def test_reject_nonofficial_source_or_key(self):
        for source in (BASE.replace("deb.debian.org", "debXdebianYorg"), BASE.replace("deb.debian.org", "example.com"), BASE.replace("trixie", "bookworm"), BASE.replace(".pgp", ".asc"), BASE.replace("debian-archive-keyring.pgp", "unrelated.pgp"), BASE + "Trusted: yes\n"):
            with self.subTest(source=source):
                self.assertNotEqual(self.parse(source).returncode, 0)

    def test_reject_missing_or_duplicate_source_fields(self):
        for source in ("", BASE.replace("Components: main\n", "", 1), BASE + "Types: deb\n"):
            with self.subTest(source=source):
                self.assertNotEqual(self.parse(source).returncode, 0)


if __name__ == "__main__":
    unittest.main()
