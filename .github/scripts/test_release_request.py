"""Check publication intent independently of GitHub and network access."""
import unittest
from release_request import release_request, version_increased, workspace_version


def manifest(version):
    return '[workspace.package]\nversion = "' + version + '"\n'


class ReleaseRequestTests(unittest.TestCase):
    def test_patch_minor_and_major_increases_request_publication(self):
        for before, after in [("0.2.0", "0.2.1"), ("0.2.9", "0.3.0"), ("0.9.9", "1.0.0"), ("0.2.9", "0.2.10")]:
            self.assertTrue(version_increased(manifest(before), manifest(after)))

    def test_ordinary_changes_do_not_request_publication(self):
        self.assertFalse(version_increased(manifest("0.2.0"), manifest("0.2.0") + '\n[profile.player]\ncodegen-units = 1\n'))

    def test_lowering_or_malformed_versions_is_refused(self):
        with self.assertRaisesRegex(ValueError, "must not lower"):
            version_increased(manifest("0.2.0"), manifest("0.1.9"))
        for version in ["v0.2.0", "0.2", "00.2.0", "0.2.0-rc.1"]:
            with self.assertRaisesRegex(ValueError, "canonical"):
                workspace_version(manifest(version))

    def test_a_master_version_merge_selects_the_exact_before_and_after(self):
        reads = []
        before = "a" * 40
        commit = "b" * 40
        def read(sha):
            reads.append(sha)
            return manifest("0.2.0")
        self.assertEqual(release_request("push", "refs/heads/master", {"before": before, "after": commit}, commit, manifest("0.2.1"), read), (True, "0.2.1"))
        self.assertEqual(reads, [before])

    def test_manual_preparation_does_not_read_history_or_publish(self):
        def unexpected(_):
            self.fail("Preparation must not request old source")
        self.assertEqual(release_request("workflow_dispatch", "refs/heads/master", {}, "b" * 40, manifest("0.2.0"), unexpected), (False, "0.2.0"))

    def test_other_branches_tags_and_events_do_not_request_publication(self):
        for event, ref in [("push", "refs/heads/topic"), ("push", "refs/tags/v0.2.1"), ("pull_request", "refs/heads/master"), ("workflow_dispatch", "refs/heads/topic")]:
            with self.assertRaises(ValueError):
                release_request(event, ref, {}, "b" * 40, manifest("0.2.1"), lambda _: manifest("0.2.0"))

    def test_missing_or_mismatched_source_history_is_refused(self):
        for before, after in [("0" * 40, "b" * 40), ("--help", "b" * 40), ("a" * 40, "c" * 40)]:
            with self.assertRaises(ValueError):
                release_request("push", "refs/heads/master", {"before": before, "after": after}, "b" * 40, manifest("0.2.1"), lambda _: manifest("0.2.0"))


if __name__ == "__main__":
    unittest.main()
