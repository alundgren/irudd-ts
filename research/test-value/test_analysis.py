import copy
import unittest

from analyze import analyze, evaluate_fault, retained, select, summarize_faults


def matrix():
    tests = [{"id": tid, "name": "same name", "file": "case.test.ts", "durationMs": 1} for tid in "ABCDE"]
    kill_sets = {
        "ab1": "AB", "ab2": "AB", "abc": "ABC", "c": "C", "d": "D", "cd": "CD", "survivor": "",
    }
    return {
        "schemaVersion": 1, "subject": {"name": "analytical control"}, "baselineComplete": True,
        "tests": tests,
        "mutants": [{"id": mid, "status": "killed" if killers else "survived",
                     "outcomes": {tid: "killed" if tid in killers else "notKilled" for tid in "ABCDE"}}
                    for mid, killers in kill_sets.items()],
    }


class AnalysisControls(unittest.TestCase):
    def test_fixed_minimal_requirements_group_duplicate_mutants_and_exclude_survivor(self):
        result = analyze(matrix())
        self.assertEqual(result["summary"]["observedMutationScore"], 6 / 7)
        self.assertEqual(result["summary"]["subsumingRequirements"], 3)
        self.assertEqual({tuple(req["killedBy"]) for req in result["requirements"]}, {("A", "B"), ("C",), ("D",)})
        grouped = next(req for req in result["requirements"] if req["killedBy"] == ["A", "B"])
        self.assertEqual(grouped["mutants"], ["ab1", "ab2"])
        self.assertEqual(result["unexplainedSurvivors"], ["survivor"])

    def test_redundant_group_has_zero_individual_loss_but_joint_removal_loses_requirement(self):
        result = analyze(matrix())
        values = {test["id"]: test for test in result["tests"]}
        self.assertEqual(values["A"]["lostSubsumingRequirementsOnRemoval"], 0)
        self.assertEqual(values["B"]["lostSubsumingRequirementsOnRemoval"], 0)
        self.assertEqual(values["C"]["lostSubsumingRequirementsOnRemoval"], 1)
        self.assertEqual(values["C"]["exclusiveMutantsKilled"], 1)
        self.assertEqual(values["E"]["redundancy"], None)
        self.assertEqual(result["redundancyGroups"][0]["tests"], ["A", "B"])
        self.assertEqual(retained(result, "BCDE"), 1)
        self.assertEqual(retained(result, "CDE"), 2 / 3)
        self.assertEqual(len(result["greedyCore"]), 3)
        self.assertEqual(retained(result, result["greedyCore"]), 1)

    def test_unknown_missing_extra_and_mixed_runtime_results_never_become_passes(self):
        for change in ["unknown", "missing", "extra", "runtime", "aggregate"]:
            with self.subTest(change=change):
                data = matrix()
                column = data["mutants"][0]
                if change == "unknown":
                    column["outcomes"]["A"] = "unknown"
                elif change == "missing":
                    del column["outcomes"]["E"]
                elif change == "extra":
                    column["outcomes"]["new test"] = "notKilled"
                elif change == "runtime":
                    column["infrastructureErrors"] = ["teardown failed"]
                else:
                    column["status"] = "survived"
                result = analyze(data)
                self.assertFalse(result["complete"])
                self.assertEqual(result["excluded"][0]["id"], "ab1")
                self.assertEqual(result["summary"]["acceptedMutants"], 6)
                with self.assertRaisesRegex(ValueError, "complete matrix"):
                    evaluate_fault(data, {"id": "fault", "verified": True, "killedBy": ["A"]})
                # Corrected inventory is accepted again.
                self.assertTrue(analyze(matrix())["complete"])

    def test_empty_kill_sets_do_not_subsume_and_zero_denominator_is_unavailable(self):
        data = matrix()
        data["mutants"] = [data["mutants"][-1]]
        result = analyze(data)
        self.assertEqual(result["summary"]["observedMutationScore"], 0)
        self.assertIsNone(result["summary"]["baselineRequirementRetention"])
        self.assertEqual(result["requirements"], [])
        self.assertEqual(result["greedyCore"], [])
        self.assertIsNone(retained(result, "ABCDE"))
        self.assertTrue(all(test["redundancy"] is None for test in result["tests"]))

    def test_invalid_mutant_separate_from_execution_error(self):
        data = matrix()
        data["mutants"].append({"id": "invalid", "status": "invalid", "invalidReason": "compilerRejected", "outcomes": {}})
        result = analyze(data)
        self.assertTrue(result["complete"])
        self.assertEqual(result["summary"]["observedMutationScore"], 6 / 7)
        del data["mutants"][-1]["invalidReason"]
        self.assertFalse(analyze(data)["complete"])
        with self.assertRaisesRegex(ValueError, "complete matrix"):
            evaluate_fault(data, {"id":"stale-plan", "verified":True,"killedBy":["A"]})
        data["mutants"][-1]["status"] = "timeout"
        self.assertFalse(analyze(data)["complete"])

    def test_duplicate_identity_or_unsafe_duration_rejected(self):
        for change in ["test", "mutant", "nan", "negative", "bool"]:
            data = matrix()
            if change == "test":
                data["tests"][1]["id"] = "A"
            elif change == "mutant":
                data["mutants"][1]["id"] = "ab1"
            else:
                data["tests"][0]["durationMs"] = {"nan": float("nan"), "negative": -1, "bool": True}[change]
            with self.subTest(change=change), self.assertRaises(ValueError):
                analyze(data)

    def test_baseline_failure_prevents_requirement_derivation(self):
        data = matrix()
        data["baselineComplete"] = False
        result = analyze(data)
        self.assertFalse(result["complete"])
        self.assertEqual(result["requirements"], [])
        self.assertIsNone(result["summary"]["observedMutationScore"])

    def test_selection_uses_equal_budgets_and_seeded_ties_without_fault_labels(self):
        data = matrix()
        original = copy.deepcopy(data)
        fault = {"id": "real-fault", "verified": True, "killedBy": ["D"], "relatedGroup": "file1"}
        evaluation = evaluate_fault(data, fault, seeds=12)
        self.assertEqual(data, original)
        analysis = analyze(data)
        for trial in evaluation["trials"]:
            self.assertEqual(len(trial["selected"]), trial["budget"])
            self.assertEqual(len(set(trial["selected"])), trial["budget"])
            self.assertEqual(trial["selected"], select(analysis, trial["budget"], trial["strategy"], trial["seed"]))
            self.assertEqual(trial["detectsFault"], "D" in trial["selected"])
            if trial["budget"] == 5:
                self.assertTrue(trial["detectsFault"])
        self.assertEqual(evaluation, evaluate_fault(data, fault, seeds=12))
        summary = summarize_faults([evaluation], bootstrap_samples=100)
        self.assertEqual(summary["faults"], 1)
        self.assertTrue(all(cell["faults"] == 1 for cell in summary["comparisons"]))
        with self.assertRaisesRegex(ValueError, "duplicate fault"):
            summarize_faults([evaluation, evaluation])

    def test_same_names_remain_distinct_and_unknown_fault_inventory_rejected(self):
        self.assertEqual(len(analyze(matrix())["tests"]), 5)
        with self.assertRaisesRegex(ValueError, "fault inventory"):
            evaluate_fault(matrix(), {"id": "bad", "verified": True, "killedBy": ["not-in-pool"]})


if __name__ == "__main__":
    unittest.main()
