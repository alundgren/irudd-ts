#!/usr/bin/env python3
"""Verify archive bytes and recorded report counts without executing fixtures."""
import collections
import hashlib
import json
import math
from pathlib import Path
import re
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def report_claim(path, data):
    claim = {"path": path, "complete": data["complete"]}
    if "summary" in data and "results" in data:
        summary = data["summary"]
        results = data["results"]
        counts = collections.Counter(row["outcome"] for row in results)
        fields = {"killed": "killed", "survived": "survived",
                  "invalidMutant": "invalidMutants", "executionError": "executionErrors",
                  "timedOut": "timedOut", "cancelled": "cancelled", "notRun": "notRun"}
        require(set(counts) <= set(fields), path + ": unknown mutation outcome")
        for outcome, field in fields.items():
            require(summary[field] == counts[outcome], path + ": incorrect " + field)
        require(len(results) + summary.get("omittedResults", 0) == summary["planned"],
                path + ": planned count does not account for retained and omitted results")
        reused = sum(row.get("reused", False) for row in results)
        executed = sum(not row.get("reused", False) and row["outcome"] not in
                       ("invalidMutant", "notRun") for row in results)
        require(summary["reused"] == reused, path + ": incorrect reused count")
        require(summary["executed"] == executed, path + ": incorrect executed count")
        if data["complete"]:
            require(data["baseline"]["outcome"] == "passed", path + ": complete with failed baseline")
            require(not data["problems"] and not summary.get("omittedProblems", 0),
                    path + ": complete with problems")
            require(not summary.get("omittedResults", 0) and not any(counts[outcome] for outcome in
                    ("executionError", "timedOut", "cancelled", "notRun")),
                    path + ": complete with incomplete outcomes")
        claim.update(kind="mutationRun", summary=summary, baseline=data["baseline"],
                     retainedResults=len(results), retainedProblems=len(data["problems"]))
    elif "pairs" in data and "functions" in data:
        claim.update(kind="duplicateReport", pairs=data["pairs"],
                     retainedFunctions=len(data["functions"]),
                     retainedProblems=len(data["problems"]), omittedEvidence=data.get("omittedEvidence"))
    else:
        return None
    return claim


def verify(root):
    manifest = json.loads((root / "manifest.json").read_bytes())
    require(manifest["schemaVersion"] == 1, "unsupported manifest version")
    entries = manifest["files"]
    names = [row["path"] for row in entries]
    require(len(names) == len(set(names)), "duplicate manifest path")
    expected = set(names) | {"manifest.json", "SHA256SUMS"}
    actual = set()
    for path in root.rglob("*"):
        require(not path.is_symlink(), "archive contains symlink: " + str(path))
        if path.is_file():
            actual.add(path.relative_to(root).as_posix())
    require(expected == actual, "inventory differs: " + str(sorted(expected ^ actual)))
    for row in entries:
        relative = Path(row["path"])
        require(not relative.is_absolute() and ".." not in relative.parts, "unsafe manifest path")
        data = (root / relative).read_bytes()
        require(len(data) == row["bytes"], row["path"] + ": byte count differs")
        require(digest(data) == row["sha256"], row["path"] + ": SHA-256 differs")
    checksums = {}
    for line in (root / "SHA256SUMS").read_text().splitlines():
        checksum, name = line.split("  ", 1)
        require(name not in checksums, "duplicate checksum path")
        checksums[name] = checksum
    require(set(checksums) == actual - {"SHA256SUMS"}, "checksum inventory differs")
    for name, checksum in checksums.items():
        require(digest((root / name).read_bytes()) == checksum, name + ": checksum list differs")

    claims = json.loads((root / "report-summary.json").read_bytes())
    expected_claims = []
    for row in entries:
        if row["category"] not in ("raw", "duplicateFixtures"):
            continue
        path = root / row["path"]
        if path.suffix not in (".json", ".stdout"):
            continue
        try:
            data = json.loads(path.read_bytes())
        except (ValueError, UnicodeDecodeError):
            continue
        if isinstance(data, dict) and "complete" in data:
            claim = report_claim(row["path"], data)
            if claim:
                expected_claims.append(claim)
    expected_claims.sort(key=lambda row: row["path"])
    require(claims["reports"] == expected_claims, "report summary differs from retained reports")

    for row in claims["sourceChecks"]:
        data = (root / row["archivedSource"]).read_bytes()
        require(digest(data) == row["expectedSha256"], "source proof mismatch: " + row["archivedSource"])
        report = json.loads((root / row["report"]).read_bytes())
        selected = {item["path"]: item for item in report["selection"]["selected"]}
        require(selected[row["selectedPath"]]["sha256"] == row["expectedSha256"],
                "report/source link differs: " + row["report"])

    sdk_controls = 0
    for summary_path in sorted((root / "proofs/sdk-eleven-to-eighteen").glob("run-*/summary.json")):
        summary = json.loads(summary_path.read_bytes())
        directory = summary_path.parent
        source = (directory / "inputs/original-sdk-mutator.ts").read_bytes()
        require(digest(source) == summary["originalSdkSha256"], "SDK proof original source differs")
        report_bytes = (directory / "inputs/r10-report.json").read_bytes()
        require(digest(report_bytes) == summary["reportSha256"], "SDK proof report differs")
        original_report = json.loads(report_bytes)
        mutations = {row["mutationId"]: row for row in original_report["results"]}
        before_bytes = (directory / "inputs/original-eleven-tests.ts").read_bytes()
        after_bytes = (directory / "inputs/corrected-eighteen-tests.ts").read_bytes()
        require(digest(before_bytes) == summary["beforeTests"]["sha256"], "SDK before-test source differs")
        require(digest(after_bytes) == summary["afterTests"]["sha256"], "SDK after-test source differs")

        def verify_control(control, source_hash, tests_hash, test_count, exit_code):
            result = json.loads((directory / control["label"] / "result.json").read_bytes())
            require(result == control, "SDK control summary differs: " + control["label"])
            require(control["sourceSha256"] == source_hash and control["testsSha256"] == tests_hash,
                    "SDK control input identity differs: " + control["label"])
            require(control["exitCode"] == exit_code and not control["timedOut"]
                    and control["matchesExpected"], "SDK control outcome differs: " + control["label"])
            for stream in ("stdout", "stderr"):
                output = (directory / control["label"] / (stream + ".txt")).read_bytes()
                details = control["output"][stream]
                require(digest(output) == details["sha256"] and len(output) == details["retainedBytes"],
                        "SDK control stream differs: " + control["label"])
                require(not details["truncated"] and len(output) == details["totalBytes"],
                        "SDK proof stream is incomplete: " + control["label"])
            stdout = (directory / control["label"] / "stdout.txt").read_text()
            tap = {name: int(count) for name, count in re.findall(
                r"^# (tests|pass|fail|cancelled|skipped|todo) (\d+)$", stdout, re.MULTILINE)}
            require(tap["tests"] == test_count and tap["cancelled"] == 0,
                    "SDK TAP test count differs: " + control["label"])
            require(tap["fail"] == 0 if exit_code == 0 else tap["fail"] > 0,
                    "SDK TAP failure count differs: " + control["label"])
            require(all(line in stdout.splitlines() for line in control["tapSummary"]),
                    "SDK TAP summary differs: " + control["label"])

        for baseline in summary["baselines"]:
            after = baseline["testsSha256"] == summary["afterTests"]["sha256"]
            tests = summary["afterTests"] if after else summary["beforeTests"]
            verify_control(baseline, digest(source), tests["sha256"], tests["count"], 0)
        ids = set()
        for proof in summary["proofs"]:
            mutation_id = proof["mutationId"]
            require(mutation_id not in ids, "duplicate SDK proof mutation")
            ids.add(mutation_id)
            mutation = mutations[mutation_id]
            require(mutation["outcome"] == "survived", "SDK proof did not start with a survivor")
            for field in ("location", "expected", "replacement", "operator"):
                require(proof[field] == mutation[field], "SDK proof edit differs: " + mutation_id)
            location = proof["location"]
            start, end = location["start"], location["end"]
            require(source[start:end] == proof["expected"].encode(), "SDK expected edit bytes differ")
            edited = source[:start] + proof["replacement"].encode() + source[end:]
            require(digest(edited) == proof["mutatedSourceSha256"], "SDK mutated source hash differs")
            verify_control(proof["old"], digest(edited), summary["beforeTests"]["sha256"],
                           summary["beforeTests"]["count"], 0)
            verify_control(proof["new"], digest(edited), summary["afterTests"]["sha256"],
                           summary["afterTests"]["count"], 1)
        require(summary["allElevenDistinguished"] and len(ids) == 11 and summary["error"] is None,
                "SDK eleven-control claim differs")
        sdk_controls += len(ids)

    host_survivors = 0
    host_path = root / "proofs/host-survivors/survivor-classification.json"
    if host_path.is_file():
        classification = json.loads(host_path.read_bytes())
        report_bytes = (root / "raw/t3-hostClassification-r10.json").read_bytes()
        original = json.loads(report_bytes)
        survivors = {row["mutationId"]: row for row in original["results"] if row["outcome"] == "survived"}
        rows = classification["survivors"]
        require(len(rows) == len(survivors) and {row["mutationId"] for row in rows} == set(survivors),
                "host classification does not account for every original survivor exactly once")
        require(classification["originalSummary"] == original["summary"] and
                classification["originalBaseline"] == original["baseline"], "host original report values differ")
        require(classification["sourceSha256"] == original["selection"]["selected"][0]["sha256"],
                "host classification source identity differs")
        counts = dict(collections.Counter(row["classification"] for row in rows))
        require(counts == classification["counts"], "host classification totals differ")
        for row in rows:
            for field in ("operator", "location", "expected", "replacement"):
                require(row[field] == survivors[row["mutationId"]][field], "host classified edit differs")
        probes = json.loads((host_path.parent / "probe-results.json").read_bytes())
        require(probes["reportSha256"] == digest(report_bytes) and
                probes["sourceSha256"] == classification["sourceSha256"], "host probe input identity differs")
        require(len(probes["results"]) == len(survivors) and
                {row["mutationId"] for row in probes["results"]} == set(survivors),
                "host probes do not account for every original survivor exactly once")
        host_survivors = len(rows)
    reporter_survivors = 0
    reporter_pairs = 0
    reporter_path = root / "proofs/reporter-survivors/survivor-classification.final.json"
    if reporter_path.is_file():
        directory = reporter_path.parent
        classification = json.loads(reporter_path.read_bytes())
        original_bytes = (root / "raw/self-reporter-final18.json").read_bytes()
        original = json.loads(original_bytes)
        require((directory / "original-full-report.json").read_bytes() == original_bytes,
                "reporter review snapshot differs from original full report")
        authoritative = classification["authoritativeReport"]
        require(authoritative["sha256"] == digest(original_bytes) and
                authoritative["summary"] == original["summary"] and
                authoritative["complete"] == original["complete"], "reporter authoritative report differs")
        mutations = {row["mutationId"]: row for row in original["results"]}
        survivors = {key: row for key, row in mutations.items() if row["outcome"] == "survived"}
        rows = classification["survivors"]
        require(len(rows) == len(survivors) and {row["mutationId"] for row in rows} == set(survivors),
                "reporter classification does not account for every survivor exactly once")
        require(dict(collections.Counter(row["classification"] for row in rows)) == classification["classificationCounts"],
                "reporter classification totals differ")
        require(dict(collections.Counter(row["outcome"] for row in original["results"])) == classification["outcomes"],
                "reporter original outcome totals differ")
        incomplete = [row for row in original["results"] if row["outcome"] not in ("killed", "survived")]
        require(classification["nonSurvivorIncompleteOutcomes"] == incomplete,
                "reporter classification changes incomplete outcomes")
        require(classification["expectedMutationCount"] == original["summary"]["planned"] and
                classification["completeRecordCount"] == sum(row["outcome"] in ("killed", "survived") for row in original["results"]),
                "reporter checkpoint coverage differs")
        for row in rows:
            for field in ("operator", "location", "expected", "replacement"):
                require(row[field] == survivors[row["mutationId"]][field], "reporter classified edit differs")
        source = (root / "sources/product-replay/sdk/mutator-vitest-reporter.ts").read_bytes()
        protocol = (root / "sources/product-replay/sdk/mutator.ts").read_bytes()
        require(digest(source) == classification["sourceReporterSha256"] and
                digest(protocol) == classification["sourceProtocolSha256"], "reporter review source identity differs")
        before_tests = (directory / "mutator_reporter.original-five.test.ts").read_bytes()
        after_tests = (directory / "mutator_reporter.final-v3.test.ts").read_bytes()
        require(after_tests == (root / "sources/delivery-tests/tests/mutator_reporter.test.ts").read_bytes(),
                "reporter final proposal differs from delivered tests")
        witnesses = json.loads((directory / "public-control-witnesses.json").read_bytes())

        def reporter_control(name, mutation_id, status, tests_bytes, variant, expected_tests=None):
            require(Path(name).name == name, "unsafe reporter proof filename")
            proof = json.loads((directory / name).read_bytes())
            require(proof["mutationId"] == mutation_id and proof["variant"] == variant and
                    proof["status"] == status and proof["signal"] is None and proof["error"] is None,
                    "reporter direct proof identity or status differs: " + name)
            require(proof["reporterBeforeSha256"] == digest(source) == proof["reporterAfterSha256"] and
                    proof["protocolSha256"] == digest(protocol) == proof["protocolAfterSha256"] and
                    proof["testInputSha256"] == digest(tests_bytes), "reporter direct proof source differs: " + name)
            if mutation_id == "baseline":
                require(proof["mutation"] is None and proof["reporterChildSha256"] == digest(source),
                        "reporter direct baseline differs")
            else:
                mutation = mutations[mutation_id]
                for field in ("mutationId", "operator", "location", "expected", "replacement"):
                    require(proof["mutation"][field] == mutation[field], "reporter proof edit differs: " + name)
                location = mutation["location"]
                start, end = location["start"], location["end"]
                require(source[start:end] == mutation["expected"].encode(), "reporter proof expected bytes differ")
                edited = source[:start] + mutation["replacement"].encode() + source[end:]
                require(proof["reporterChildSha256"] == digest(edited), "reporter proof mutated source differs")
            counts = {name: int(count) for name, count in re.findall(
                r"^ℹ (tests|pass|fail|cancelled|skipped|todo) (\d+)$", proof["stdout"], re.MULTILINE)}
            require(counts["tests"] > 0 and counts["cancelled"] == 0 and counts["skipped"] == 0,
                    "reporter direct proof has incomplete test execution: " + name)
            if expected_tests is not None:
                require(counts["tests"] == expected_tests, "reporter direct proof test count differs: " + name)
            require(counts["fail"] == 0 and counts["pass"] == counts["tests"] if status == 0 else counts["fail"] > 0,
                    "reporter direct proof test outcome differs: " + name)

        baseline = witnesses["baselineFinalV3"]
        require(baseline["status"] == 0 and baseline["tests"] == 12 and baseline["sha256"] == digest(after_tests),
                "reporter V3 baseline claim differs")
        reporter_control(baseline["file"], "baseline", 0, after_tests, "final3", 12)
        ids = set()
        for witness in witnesses["witnesses"]:
            mutation_id = witness["mutationId"]
            require(mutation_id in survivors and mutation_id not in ids, "reporter public witness is not a distinct survivor")
            ids.add(mutation_id)
            old, new = witness["oldOriginalFive"], witness["newFinalV3Control"]
            require(old["status"] == 0 and new["status"] == 1, "reporter witness status claim differs")
            reporter_control(old["file"], mutation_id, 0, before_tests, "original", 5)
            reporter_control(new["file"], mutation_id, 1, after_tests, "final3")
        require(len(ids) == 8, "reporter public witness pair count differs")
        timeout_witness = witnesses["separateTimedOutSiteWitness"]
        timeout_id = timeout_witness["mutationId"]
        require(mutations[timeout_id]["outcome"] == timeout_witness["recordedMutationOutcome"] == "timedOut",
                "reporter separate timeout witness changes original outcome")
        reporter_control(timeout_witness["oldOriginalFive"], timeout_id, 0, before_tests, "original", 5)
        reporter_control(timeout_witness["newFinalV3Control"], timeout_id, 1, after_tests, "final3")
        reporter_survivors, reporter_pairs = len(rows), len(ids)
        followup_path = directory / "unchanged-reuse-followup/survivor-classification.followup-88.json"
        if followup_path.is_file():
            followup = json.loads(followup_path.read_bytes())
            followup_bytes = (root / "raw/self-reporter-final18-unchanged-reuse.json").read_bytes()
            followup_report = json.loads(followup_bytes)
            require((followup_path.parent / "unchanged-reuse-report.json").read_bytes() == followup_bytes,
                    "reporter follow-up snapshot differs")
            require(followup["authoritativeReport"]["sha256"] == digest(followup_bytes) and
                    followup["authoritativeReport"]["summary"] == followup_report["summary"] and
                    followup["authoritativeReport"]["complete"] == followup_report["complete"],
                    "reporter follow-up authoritative values differ")
            require(followup_report["inputDigest"] == original["inputDigest"] == followup["inputDigest"],
                    "reporter unchanged-input digest differs")
            current = {row["mutationId"]: row for row in followup_report["results"]}
            classified = {row["mutationId"]: row for row in followup["survivors"]}
            require(len(classified) == len(followup["survivors"]) and set(classified) ==
                    {key for key, row in current.items() if row["outcome"] == "survived"},
                    "reporter follow-up survivor accounting differs")
            require(dict(collections.Counter(row["classification"] for row in classified.values())) == followup["classificationCounts"],
                    "reporter follow-up classification totals differ")
            require(followup["originalReport"]["sha256"] == digest(original_bytes) and
                    followup["originalClassification"]["sha256"] == digest(reporter_path.read_bytes()),
                    "reporter follow-up original identities differ")
            for row in rows:
                require(classified[row["mutationId"]] == row, "reporter follow-up changes an original survivor classification")
            added = set(classified) - set(survivors)
            require(added == set(followup["additionalSurvivorIds"]) == {row["mutationId"] for row in incomplete},
                    "reporter follow-up added survivor IDs differ")
            for mutation_id, row in current.items():
                previous = mutations[mutation_id]
                for field in ("operator", "location", "expected", "replacement"):
                    require(row[field] == previous[field], "reporter follow-up edit differs")
                if previous["outcome"] in ("killed", "survived"):
                    evidence = {key: value for key, value in row.items() if key not in ("reused", "elapsedMs")}
                    old_evidence = {key: value for key, value in previous.items() if key not in ("reused", "elapsedMs")}
                    require(row["reused"] and evidence == old_evidence and
                            math.isclose(row["elapsedMs"], previous["elapsedMs"], rel_tol=1e-15, abs_tol=1e-9),
                            "reporter reused evidence differs from original record")
                else:
                    require(not row["reused"] and row["outcome"] == "survived", "reporter timeout was not retried")
            reporter_survivors = len(classified)

    print(f"Verified {len(entries)} retained files, checksum inventory, "
          f"{len(expected_claims)} report summaries, {len(claims['sourceChecks'])} source links "
          f"and {sdk_controls} SDK correction controls; {host_survivors} host and {reporter_survivors} reporter "
          f"classification rows, {reporter_pairs} reporter proof pairs and a separate timeout witness checked.")


if __name__ == "__main__":
    try:
        verify(Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print("Archive verification failed: " + str(error), file=sys.stderr)
        sys.exit(1)
