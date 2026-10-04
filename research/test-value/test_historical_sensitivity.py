import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import historical_sensitivity as historical
from sensitivity import derive, vitest_outcomes


CONTROL_CANDIDATE = 't3code-151c241cd2cc'


@unittest.skipUnless(
    os.environ.get('TEST_VALUE_HISTORICAL_ROOT') and os.environ.get('TEST_VALUE_HISTORICAL_PLAN'),
    'retained original-history evidence and reviewed v2 plan were not supplied',
)
class HistoricalSensitivityControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.primary_root = Path(os.environ['TEST_VALUE_HISTORICAL_ROOT']).resolve(strict=True)
        cls.plan_path = Path(os.environ['TEST_VALUE_HISTORICAL_PLAN']).resolve(strict=True)
        cls.primary = historical.load_archive_inputs(cls.primary_root)
        (
            cls.primary_root,
            cls.manifest_path,
            cls.manifest_bytes,
            cls.manifest_digest,
            cls.archive_path,
            cls.archive_manifest,
            cls.files,
            cls.attempts,
            cls.preregistration,
        ) = cls.primary
        cls.attempt_by_id = {row['id']: row for row in cls.attempts}
        cls.registered = historical.candidate_registration(cls.preregistration)
        if CONTROL_CANDIDATE not in cls.attempt_by_id:
            raise AssertionError('retained control candidate is absent')
        cls.archive_sha = historical.sha_file(cls.archive_path)
        cls.manifest_sha = historical.sha_file(cls.manifest_path)

    def copy_primary_root(self, destination):
        destination = Path(destination)
        source_manifest = historical.parse_json(self.manifest_bytes, self.manifest_path)
        archive_relative = historical.relative_name(source_manifest['archive'])
        archive_copy = destination / archive_relative
        archive_copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(self.archive_path, archive_copy)
        (destination / 'manifest.json').write_bytes(self.manifest_bytes)
        return destination

    def extract(self, scratch):
        roots = historical.extract_registered_roots(
            scratch, self.archive_path, self.files, self.attempts
        )
        self.assertIn(CONTROL_CANDIDATE, roots)
        return Path(scratch) / CONTROL_CANDIDATE

    def control_fault(self, candidate_root):
        attempt = self.attempt_by_id[CONTROL_CANDIDATE]
        fault = historical.read_json(candidate_root / 'fault.json')
        registered = self.registered[CONTROL_CANDIDATE]
        return attempt, fault, registered

    def test_retained_test_body_failure_remains_separate_from_assertions(self):
        with tempfile.TemporaryDirectory(prefix='historical-secondary-control-') as scratch:
            candidate_root = self.extract(scratch)
            attempt, fault, registered = self.control_fault(candidate_root)
            binding = historical.validate_events(
                candidate_root,
                attempt,
                fault,
                registered,
                self.preregistration,
                self.files,
            )
            self.assertEqual(binding['baselinePath'], 'fixed-before')
            self.assertIsNone(binding['matrix']['provenance'].get('node'))
            self.assertEqual(binding['frozenNode'], self.preregistration['node'][attempt['subject']])
            self.assertEqual(binding['declaredPrefixCount'], 24)
            self.assertEqual(len(binding['eventDigests']), 27)

            original_matrix = (candidate_root / 'matrix.json').read_bytes()
            original_fault = (candidate_root / 'fault.json').read_bytes()
            original_labels = historical.canonical_sha({
                'verified': fault['verified'],
                'killedBy': fault['killedBy'],
                'relatedGroup': fault.get('relatedGroup'),
            })
            body_failure_column = None
            body_failure_categories = None
            for mutant in binding['matrix']['mutants']:
                event_root = candidate_root / Path(mutant['evidence']).parent
                event = historical.read_json(event_root / 'execution.json')
                if any(
                    failure.get('kind') == 'runtime'
                    for failure in event.get('protocol', {}).get('failures', [])
                ):
                    body_failure_column = mutant
                    _, body_failure_categories = vitest_outcomes(
                        event_root,
                        binding['matrix']['tests'],
                        binding['frozenNode'],
                    )
                    break
            self.assertIsNotNone(body_failure_column)
            self.assertTrue(body_failure_categories['otherFailureTests'])

            with tempfile.TemporaryDirectory(prefix='historical-secondary-derived-') as derived:
                derive(candidate_root, Path(derived) / 'derived')
                derived_matrix = historical.read_json(Path(derived) / 'derived/matrix.json')
                derived_column = next(
                    row for row in derived_matrix['mutants'] if row['id'] == body_failure_column['id']
                )
                for test_id in body_failure_categories['otherFailureTests']:
                    self.assertEqual(derived_column['outcomes'][test_id], 'killed')

            self.assertEqual((candidate_root / 'matrix.json').read_bytes(), original_matrix)
            self.assertEqual((candidate_root / 'fault.json').read_bytes(), original_fault)
            self.assertEqual(
                historical.canonical_sha({
                    'verified': fault['verified'],
                    'killedBy': fault['killedBy'],
                    'relatedGroup': fault.get('relatedGroup'),
                }),
                original_labels,
            )

    def test_stale_fault_event_runtime_and_short_prefix_are_rejected(self):
        with tempfile.TemporaryDirectory(prefix='historical-secondary-stale-fault-') as scratch:
            candidate_root = self.extract(scratch)
            attempt, fault, registered = self.control_fault(candidate_root)
            changed_fault = copy.deepcopy(fault)
            changed_fault['killedBy'] = list(changed_fault['killedBy']) + ['invented test label']
            with self.assertRaisesRegex(historical.EvidenceError, 'differ for killedBy'):
                historical.require_core_match(attempt, changed_fault, registered)
            historical.validate_attempt_registration(self.attempts, self.registered)
            orphaned_attempts = self.attempts + [{**self.attempts[0], 'id': 'not-preregistered'}]
            with self.assertRaisesRegex(historical.EvidenceError, 'absent from preregistration'):
                historical.validate_attempt_registration(orphaned_attempts, self.registered)

        with tempfile.TemporaryDirectory(prefix='historical-secondary-stale-event-') as scratch:
            candidate_root = self.extract(scratch)
            attempt, fault, registered = self.control_fault(candidate_root)
            event_path = candidate_root / 'mutant-0001/execution.json'
            event_path.write_bytes(event_path.read_bytes() + b'\n')
            with self.assertRaisesRegex(historical.EvidenceError, 'execution event digest disagrees'):
                historical.validate_events(
                    candidate_root,
                    attempt,
                    fault,
                    registered,
                    self.preregistration,
                    self.files,
                )

        with tempfile.TemporaryDirectory(prefix='historical-secondary-stale-runtime-') as scratch:
            candidate_root = self.extract(scratch)
            attempt, _, _ = self.control_fault(candidate_root)
            frozen_node = self.preregistration['node'][attempt['subject']]
            baseline = historical.read_json(candidate_root / 'fixed-before/execution.json')
            baseline_input = baseline['request']['inputDigest']
            changed_runtime = copy.deepcopy(baseline)
            changed_runtime['node']['sha256'] = '0' * 64
            with self.assertRaisesRegex(historical.EvidenceError, 'runtime identity changed'):
                historical.validate_runtime_event(
                    changed_runtime,
                    frozen_node,
                    baseline_input,
                    False,
                    'fixed-before/execution.json',
                )

        with tempfile.TemporaryDirectory(prefix='historical-secondary-short-prefix-') as scratch:
            candidate_root = self.extract(scratch)
            matrix = historical.read_json(candidate_root / 'matrix.json')
            plan = historical.read_json(candidate_root / 'plan.json')
            shortened = copy.deepcopy(matrix)
            shortened['mutants'].pop()
            with self.assertRaisesRegex(historical.EvidenceError, 'expected 24, found 23'):
                historical.validate_declared_prefix(
                    shortened,
                    plan,
                    self.preregistration['mutantPrefixLimit'],
                )

    def test_complete_offline_cohort_preserves_primary_labels_and_inputs(self):
        attempts_sha = self.files['attempts.json']['sha256']
        preregistration_sha = self.files['preregistration.json']['sha256']
        with tempfile.TemporaryDirectory(prefix='historical-secondary-run-') as parent:
            output = Path(parent) / 'result'
            result = historical.run_secondary(self.plan_path, self.primary_root, output)
            self.assertEqual(result['summary']['primaryAssertionEligible'], {'faults': 3, 'relatedGroups': 3})
            self.assertEqual(result['summary']['secondaryEligible'], {'faults': 5, 'relatedGroups': 5})
            self.assertEqual(result['summary']['pairedEligible'], {'faults': 3, 'relatedGroups': 3})
            self.assertEqual(result['summary']['additionalSecondaryEligible'], {'faults': 2, 'relatedGroups': 2})
            self.assertEqual(result['summary']['primaryVerifiedAttempts'], 5)
            self.assertEqual(result['summary']['primaryUnverifiedAttemptsKeptOut'], 39)
            by_id = {row['id']: row for row in result['candidates']}
            row = by_id[CONTROL_CANDIDATE]
            self.assertEqual(row['fixedBaselinePath'], 'fixed-before')
            self.assertEqual(row['runtime'], self.preregistration['node'][row['subject']])
            self.assertEqual(
                row['primaryFaultLabels'],
                self.attempt_by_id[CONTROL_CANDIDATE]['killedBy'],
            )
            self.assertEqual(len(row['eventDigests']), 27)
            self.assertEqual(
                historical.sha_file(self.archive_path),
                self.archive_sha,
            )
            self.assertEqual(historical.sha_file(self.manifest_path), self.manifest_sha)
            self.assertEqual(self.files['attempts.json']['sha256'], attempts_sha)
            self.assertEqual(self.files['preregistration.json']['sha256'], preregistration_sha)
            self.assertEqual(
                json.loads((output / 'aggregates.json').read_text())['primary'],
                json.loads((output / 'aggregates.json').read_text())['pairedPrimary'],
            )

    def test_existing_and_dangling_symlink_outputs_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix='historical-secondary-output-') as parent:
            parent = Path(parent)
            existing = parent / 'existing'
            existing.mkdir()
            with self.assertRaisesRegex(historical.EvidenceError, 'output path already exists'):
                historical.run_secondary(self.plan_path, self.primary_root, existing)
            dangling = parent / 'dangling'
            dangling.symlink_to(parent / 'missing', target_is_directory=True)
            with self.assertRaisesRegex(historical.EvidenceError, 'output path already exists'):
                historical.run_secondary(self.plan_path, self.primary_root, dangling)
            self.assertTrue(dangling.is_symlink())
            self.assertFalse((parent / 'missing').exists())

    def test_plan_and_manifest_changes_during_analysis_are_rejected(self):
        original_evaluate = historical.evaluate_fault
        for changed_input in ('plan', 'manifest'):
            with self.subTest(changed_input=changed_input), tempfile.TemporaryDirectory(
                prefix='historical-secondary-input-change-'
            ) as parent:
                parent = Path(parent)
                primary_copy = self.copy_primary_root(parent / 'primary')
                plan_copy = parent / 'plan.json'
                shutil.copyfile(self.plan_path, plan_copy)
                if changed_input == 'plan':
                    changed_path = plan_copy
                else:
                    changed_path = primary_copy / 'manifest.json'
                changed = False

                def change_input_after_start(*args, **kwargs):
                    nonlocal changed
                    if not changed:
                        changed_path.write_bytes(changed_path.read_bytes() + b'\n')
                        changed = True
                    return original_evaluate(*args, **kwargs)

                with patch.object(historical, 'evaluate_fault', side_effect=change_input_after_start):
                    with self.assertRaisesRegex(
                        historical.EvidenceError,
                        'analysis input changed during processing',
                    ):
                        historical.run_secondary(
                            plan_copy,
                            primary_copy,
                            parent / 'output',
                        )


class StrictJsonControls(unittest.TestCase):
    def test_duplicate_fields_and_over_budget_json_are_rejected(self):
        with tempfile.TemporaryDirectory(prefix='historical-secondary-json-') as directory:
            path = Path(directory) / 'document.json'
            path.write_text('{"value":1,"value":2}', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'Duplicate JSON field: value'):
                historical.read_json(path)
            path.write_bytes(b' ' * (historical.MAX_JSON + 1))
            with self.assertRaisesRegex(ValueError, 'JSON exceeds byte budget'):
                historical.read_json(path)


if __name__ == '__main__':
    unittest.main()
