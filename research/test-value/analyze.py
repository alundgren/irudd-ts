#!/usr/bin/env python3
"""Analyze a complete observed kill matrix without interpreting it as bug probability."""

import argparse
import hashlib
import json
import math
import random
from collections import defaultdict
from pathlib import Path


VALID = {"killed", "survived"}
STATUSES = VALID | {"invalid", "error", "timeout", "cancelled", "notRun"}


def ratio(numerator, denominator):
    return numerator / denominator if denominator else None


def analyze(matrix):
    if matrix.get("schemaVersion") != 1:
        raise ValueError("unsupported matrix version")
    if not isinstance(matrix.get("baselineComplete"), bool):
        raise ValueError("baseline completeness is required")
    tests = matrix["tests"]
    ids = [test["id"] for test in tests]
    if not ids or any(not isinstance(value, str) or not value for value in ids):
        raise ValueError("a nonempty baseline test inventory is required")
    if len(set(ids)) != len(ids):
        raise ValueError("duplicate baseline test ID")
    for test in tests:
        duration = test.get("durationMs")
        if duration is not None and (
            isinstance(duration, bool) or not isinstance(duration, (int, float))
            or not math.isfinite(duration) or duration < 0
        ):
            raise ValueError("test duration must be finite and nonnegative")
    inventory = set(ids)
    seen = set()
    accepted = {}
    excluded = []
    survivors = []
    for mutant in matrix["mutants"]:
        mid = mutant["id"]
        if not isinstance(mid, str) or not mid or mid in seen:
            raise ValueError("missing or duplicate mutant ID")
        seen.add(mid)
        status = mutant["status"]
        if status not in STATUSES:
            raise ValueError("unsupported mutant status")
        outcomes = mutant.get("outcomes", {})
        if not isinstance(outcomes, dict) or any(
            value not in {"killed", "notKilled", "unknown"} for value in outcomes.values()
        ):
            raise ValueError("unsupported cell outcome")
        reason = None
        if not matrix["baselineComplete"]:
            reason = "baseline incomplete"
        elif status not in VALID:
            reason = "compilerRejected" if status == "invalid" and mutant.get("invalidReason") == "compilerRejected" else status
        elif set(outcomes) != inventory or "unknown" in outcomes.values():
            reason = "incomplete test inventory"
        elif mutant.get("infrastructureErrors"):
            reason = "infrastructure error alongside test results"
        killers = frozenset(test for test, value in outcomes.items() if value == "killed")
        if reason is None and (bool(killers) != (status == "killed")):
            reason = "aggregate outcome disagrees with test outcomes"
        if reason:
            excluded.append({"id": mid, "reason": reason})
        else:
            accepted[mid] = killers
            if not killers:
                survivors.append(mid)

    signatures = defaultdict(list)
    for mid, killers in accepted.items():
        if killers:
            signatures[killers].append(mid)
    requirements = []
    for killers, mutants in signatures.items():
        if any(other < killers for other in signatures):
            continue
        members = sorted(mutants)
        signature = json.dumps(sorted(killers), separators=(",", ":"))
        requirements.append({
            "id": hashlib.sha256(signature.encode()).hexdigest(),
            "representative": members[0], "mutants": members, "killedBy": sorted(killers),
        })
    requirements.sort(key=lambda requirement: requirement["id"])
    root_sets = {requirement["id"]: frozenset(requirement["killedBy"]) for requirement in requirements}
    kills = {tid: {mid for mid, killers in accepted.items() if tid in killers} for tid in ids}
    roots = {tid: {rid for rid, killers in root_sets.items() if tid in killers} for tid in ids}
    values = []
    for test in tests:
        tid = test["id"]
        exclusive = sum(accepted[mid] == {tid} for mid in kills[tid])
        exclusive_roots = sum(root_sets[rid] == {tid} for rid in roots[tid])
        overlaps = []
        for other in ids:
            if other == tid:
                continue
            union = roots[tid] | roots[other]
            if union:
                overlaps.append({"testId": other, "jaccard": len(roots[tid] & roots[other]) / len(union)})
        overlaps.sort(key=lambda overlap: (-overlap["jaccard"], overlap["testId"]))
        values.append({
            **test, "mutantsKilled": len(kills[tid]),
            "subsumingRequirementsKilled": len(roots[tid]),
            "exclusiveMutantsKilled": exclusive,
            "exclusiveSubsumingRequirementsKilled": exclusive_roots,
            "lostMutantKillsOnRemoval": exclusive,
            "lostSubsumingRequirementsOnRemoval": exclusive_roots,
            "marginalObservedMutationScore": ratio(exclusive, len(accepted)),
            "marginalBaselineRequirementRetention": ratio(exclusive_roots, len(requirements)),
            "redundancy": 1 - exclusive_roots / len(roots[tid]) if roots[tid] else None,
            "overlaps": overlaps,
        })
    groups = defaultdict(list)
    for tid in ids:
        if roots[tid]:
            groups[tuple(sorted(roots[tid]))].append(tid)
    redundancy_groups = [
        {"tests": sorted(members), "requirements": list(profile)}
        for profile, members in groups.items() if len(members) > 1
    ]
    covered = set()
    core = []
    while len(covered) < len(requirements):
        tid = min(ids, key=lambda value: (-len(roots[value] - covered), value))
        gain = roots[tid] - covered
        if not gain:
            raise ValueError("baseline requirement cannot be covered")
        core.append(tid)
        covered |= gain
    complete = matrix["baselineComplete"] and not any(
        value["reason"] != "compilerRejected" for value in excluded
    )
    return {
        "schemaVersion": 1, "subject": matrix.get("subject"),
        "baselineTestInventoryDigest": hashlib.sha256(json.dumps(sorted(ids)).encode()).hexdigest(),
        "baselineMatrixDigest": hashlib.sha256(json.dumps(matrix, sort_keys=True, allow_nan=False).encode()).hexdigest(),
        "baselineComplete": matrix["baselineComplete"], "complete": complete,
        "summary": {
            "tests": len(ids), "plannedMutants": len(seen), "acceptedMutants": len(accepted),
            "killedMutants": len(accepted) - len(survivors), "unexplainedSurvivors": len(survivors),
            "observedMutationScore": ratio(len(accepted) - len(survivors), len(accepted)),
            "subsumingRequirements": len(requirements),
            "baselineRequirementRetention": 1.0 if requirements else None,
            "observedKillSignatures": len(signatures), "excludedMutants": len(excluded),
            "greedyCoreTests": len(core),
        },
        "requirements": requirements, "tests": values, "redundancyGroups": redundancy_groups,
        "greedyCore": core, "unexplainedSurvivors": survivors, "excluded": excluded,
        "interpretation": {
            "requirements": "Minimal nonempty observed kill-signature classes; fixed baseline pool.",
            "retention": "The full baseline retains 100% by construction, when requirements exist.",
            "mutationScore": "Completed killed / completed killed plus surviving mutants; equivalence unknown.",
            "cost": "Per-test durations are descriptive and exclude shared setup and runner startup.",
            "limits": "Observed redundancy does not authorize deleting tests or predict bug probability.",
        },
    }


def select(analysis, count, strategy, seed):
    tests = analysis["tests"]
    if not 0 <= count <= len(tests):
        raise ValueError("selection budget is outside the test inventory")
    ordered = list(tests)
    random.Random(seed).shuffle(ordered)
    fields = {
        "raw": "mutantsKilled", "subsuming": "subsumingRequirementsKilled",
        "marginalRaw": "exclusiveMutantsKilled",
        "marginalSubsuming": "exclusiveSubsumingRequirementsKilled",
    }
    if strategy == "random":
        return [test["id"] for test in ordered[:count]]
    if strategy in fields:
        ordered.sort(key=lambda test: -test[fields[strategy]])
        return [test["id"] for test in ordered[:count]]
    if strategy == "greedySubsuming":
        profiles = {test["id"]: set() for test in tests}
        for requirement in analysis["requirements"]:
            for tid in requirement["killedBy"]:
                profiles[tid].add(requirement["id"])
        chosen = []
        covered = set()
        for _ in range(count):
            best = max(ordered, key=lambda test: len(profiles[test["id"]] - covered))
            ordered.remove(best)
            chosen.append(best["id"])
            covered |= profiles[best["id"]]
        return chosen
    raise ValueError("unknown selector")


def retained(analysis, chosen):
    chosen = set(chosen)
    return ratio(sum(bool(chosen & set(req["killedBy"])) for req in analysis["requirements"]), len(analysis["requirements"]))


def evaluate_fault(matrix, fault, seeds=100):
    analysis = analyze(matrix)
    if not analysis["complete"]:
        raise ValueError("historical selection requires a complete matrix")
    if not fault.get("verified") or not fault.get("killedBy"):
        raise ValueError("historical fault must have verified detecting tests")
    ids = {test["id"] for test in analysis["tests"]}
    killers = set(fault["killedBy"])
    if not killers <= ids:
        raise ValueError("fault inventory disagrees with the fixed test pool")
    if not isinstance(seeds, int) or isinstance(seeds, bool) or seeds < 1:
        raise ValueError("seed count must be positive")
    budgets = [(fraction, max(1, math.ceil(len(ids) * fraction))) for fraction in [0.25, 0.5, 0.75, 1]]
    strategies = ["random", "raw", "subsuming", "marginalRaw", "marginalSubsuming", "greedySubsuming"]
    trials = []
    for fraction, budget in budgets:
        for seed in range(seeds):
            for strategy in strategies:
                # Selection sees only mutation data. Fault labels are applied afterward.
                chosen = select(analysis, budget, strategy, seed)
                trials.append({
                    "requestedTestFraction": fraction, "budget": budget,
                    "seed": seed, "strategy": strategy, "selected": chosen,
                    "detectsFault": bool(killers & set(chosen)),
                    "baselineRequirementRetention": retained(analysis, chosen),
                })
    return {"schemaVersion": 1, "fault": fault, "tests": len(ids), "trials": trials}


def selection_digest(selected, trial):
    bound = {key: trial[key] for key in ["requestedTestFraction", "budget", "seed", "strategy"]}
    bound["selected"] = selected
    return hashlib.sha256(json.dumps(bound, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()


def compact_evaluation(evaluation, matrix):
    """Keep reproducible selections without repeating long test IDs in every trial."""
    analysis = analyze(matrix)
    trials = []
    for trial in evaluation["trials"]:
        selected = trial["selected"]
        trials.append({**{key: value for key, value in trial.items() if key != "selected"},
                       "selectedCount": len(selected), "selectedSha256": selection_digest(selected, trial)})
    return {**evaluation, "trials": trials, "selectionProvenance": {
        "baselineMatrixDigest": analysis["baselineMatrixDigest"],
        "baselineTestInventoryDigest": analysis["baselineTestInventoryDigest"],
        "selectorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "encoding": "SHA-256 of UTF-8 sorted-key compact JSON containing ordered selected IDs, fraction, budget, seed and strategy",
    }}


def reconstruct_selections(evaluation, matrix):
    analysis = analyze(matrix)
    provenance = evaluation["selectionProvenance"]
    for key in ["baselineMatrixDigest", "baselineTestInventoryDigest"]:
        if provenance[key] != analysis[key]:
            raise ValueError("selection matrix or inventory identity changed")
    if provenance["selectorSha256"] != hashlib.sha256(Path(__file__).read_bytes()).hexdigest():
        raise ValueError("selection implementation identity changed")
    trials = []
    for trial in evaluation["trials"]:
        selected = select(analysis, trial["budget"], trial["strategy"], trial["seed"])
        if len(selected) != trial["selectedCount"] or selection_digest(selected, trial) != trial["selectedSha256"]:
            raise ValueError("selection evidence disagrees with reconstructed trial")
        trials.append({**{key: value for key, value in trial.items() if key not in {"selectedCount", "selectedSha256"}},
                       "selected": selected})
    return {key: value for key, value in {**evaluation, "trials": trials}.items() if key != "selectionProvenance"}


def summarize_faults(evaluations, bootstrap_samples=1000):
    seen = set()
    observations = defaultdict(list)
    for evaluation in evaluations:
        fault = evaluation["fault"]
        fid = fault["id"]
        if fid in seen:
            raise ValueError("duplicate fault ID, seeds are not independent faults")
        seen.add(fid)
        cells = defaultdict(list)
        for trial in evaluation["trials"]:
            cells[(trial["requestedTestFraction"], trial["strategy"])].append(int(trial["detectsFault"]))
        for (fraction, strategy), results in cells.items():
            observations[(fraction, strategy)].append({
                "faultId": fid, "cluster": fault.get("relatedGroup", fid),
                "detectionFraction": sum(results) / len(results),
            })
    summary = []
    for (fraction, strategy), values in sorted(observations.items()):
        by_cluster = defaultdict(list)
        for value in values:
            by_cluster[value["cluster"]].append(value["detectionFraction"])
        clusters = list(by_cluster.values())
        samples = []
        rng = random.Random(20261003)
        for _ in range(bootstrap_samples):
            sample = [result for _ in clusters for result in rng.choice(clusters)]
            samples.append(sum(sample) / len(sample))
        samples.sort()
        summary.append({
            "testFraction": fraction, "strategy": strategy, "faults": len(values),
            "relatedGroups": len(clusters),
            "meanFaultDetection": sum(value["detectionFraction"] for value in values) / len(values),
            "clusterBootstrap95": [samples[int(0.025 * len(samples))], samples[min(len(samples) - 1, int(0.975 * len(samples)))]],
        })
    return {"faults": len(seen), "comparisons": summary,
            "limits": "Descriptive retrospective cohort; uncertainty resamples related-fault groups, not random seeds."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("matrix", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fault", type=Path)
    parser.add_argument("--seeds", type=int, default=100)
    args = parser.parse_args()
    matrix = json.loads(args.matrix.read_text())
    result = evaluate_fault(matrix, json.loads(args.fault.read_text()), args.seeds) if args.fault else analyze(matrix)
    encoded = json.dumps(result, indent=2, allow_nan=False) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded)
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
