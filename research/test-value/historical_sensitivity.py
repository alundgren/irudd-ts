#!/usr/bin/env python3
"""Derive a separate test-failure sensitivity cohort from registered replay records."""

import argparse
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
import tempfile

from analyze import analyze, compact_evaluation, evaluate_fault, summarize_faults
from runner import MAX_JSON, hash_tree, parse_json, read_json as runner_read_json
from sensitivity import POLICY, derive

PLAN_SHA256 = '968d50cf0c0ec877b38f2d820fb44000b1a954a611311c034f70598acad7e712'
POLICY_SEED_COUNT = 100
POLICY_FRACTIONS = [0.25, 0.5, 0.75, 1]
POLICY_STRATEGIES = ['random', 'raw', 'subsuming', 'marginalRaw', 'marginalSubsuming', 'greedySubsuming']
CORE_FIELDS = ['id', 'subject', 'repository', 'title', 'sourceFiles', 'testFiles', 'relatedGroup', 'fix', 'parent']
DIGEST_REQUIRED = ['matrix.json', 'analysis.json', 'fixed-before/execution.json', 'faulty/execution.json', 'fixed-after/execution.json']
ANALYSIS_MODULES = ['historical_sensitivity.py', 'analyze.py', 'sensitivity.py', 'runner.py', 'rust_slice.py']


class EvidenceError(ValueError):
    """A candidate's retained identity or events do not match its registration."""


def sha_bytes(value):
    return hashlib.sha256(value).hexdigest()


def sha_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_sha(value):
    return sha_bytes(json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode())


def read_json(path):
    return runner_read_json(Path(path))


def analysis_module_digests():
    root = Path(__file__).resolve().parent
    return {name: sha_file(root / name) for name in ANALYSIS_MODULES}


def write_json_safe(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n', encoding='utf-8')


def validate_plan(document, digest):
    if digest != PLAN_SHA256:
        raise EvidenceError('registered sensitivity plan hash does not match v2')
    if document.get('schemaVersion') != 1:
        raise EvidenceError('unsupported sensitivity plan version')
    if document.get('primaryPolicy') != 'assertion-only, unchanged' or document.get('secondaryPolicy') != 'posthoc-test-failures-v1, reused from the separate current-suite sensitivity analysis':
        raise EvidenceError('primary or secondary failure policy differs from the reviewed plan')
    expected_rule = 'Keep the primary assertion-regression eligibility and detecting-test labels; do not add fault labels from exceptions or reclassify excluded historical changes as verified.'
    if document.get('faultLabels') != expected_rule:
        raise EvidenceError('primary fault-label rule differs from the reviewed plan')
    comparisons = document.get('comparisons', {})
    if comparisons.get('seedCount') != POLICY_SEED_COUNT or comparisons.get('testFractions') != POLICY_FRACTIONS or comparisons.get('strategies') != POLICY_STRATEGIES:
        raise EvidenceError('selection settings differ from the reviewed plan')
    if not isinstance(document.get('candidateManifest'), dict) or len(document['candidateManifest'].get('sha256', '')) != 64:
        raise EvidenceError('candidate registration hash is missing')
    return document


def relative_name(value):
    if not isinstance(value, str) or not value:
        raise EvidenceError('empty archive path')
    path = PurePosixPath(value)
    if path.is_absolute() or '..' in path.parts or path.as_posix() != value or '\\' in value:
        raise EvidenceError('unsafe archive path: ' + repr(value))
    return value


def manifest_files(manifest):
    rows = manifest.get('files')
    if not isinstance(rows, list):
        raise EvidenceError('archive manifest has no file inventory')
    result = {}
    for row in rows:
        path = relative_name(row.get('path'))
        digest = row.get('sha256')
        size = row.get('bytes')
        if path in result or not isinstance(digest, str) or len(digest) != 64 or type(size) is not int or size < 0:
            raise EvidenceError('invalid or duplicate archived file record: ' + path)
        result[path] = row
    return result


def archive_bytes(tf, members, expected, files):
    member = members.get(expected)
    if member is None or not member.isfile():
        raise EvidenceError('required regular file is absent from archive: ' + expected)
    info = files[expected]
    if member.size != info['bytes'] or member.size > MAX_JSON:
        raise EvidenceError('archive member size disagrees with manifest: ' + expected)
    stream = tf.extractfile(member)
    if stream is None:
        raise EvidenceError('archive member cannot be read: ' + expected)
    value = stream.read(info['bytes'] + 1)
    if len(value) != info['bytes'] or sha_bytes(value) != info['sha256']:
        raise EvidenceError('archive member digest disagrees with manifest: ' + expected)
    return value


def load_archive_inputs(primary_root):
    """Read only the registered primary archive and return its manifest records."""
    primary_root = Path(primary_root).resolve(strict=True)
    manifest_path = primary_root / 'manifest.json'
    manifest_bytes = manifest_path.read_bytes()
    manifest_sha = sha_bytes(manifest_bytes)
    archive_manifest = parse_json(manifest_bytes, manifest_path)
    archive_path_value = relative_name(archive_manifest.get('archive'))
    archive_path = (primary_root / archive_path_value).resolve(strict=True)
    if not archive_path.is_relative_to(primary_root):
        raise EvidenceError('archive path leaves primary evidence directory')
    if sha_file(archive_path) != archive_manifest.get('archiveSha256'):
        raise EvidenceError('primary evidence archive digest mismatch')
    files = manifest_files(archive_manifest)
    if not {'attempts.json', 'preregistration.json'} <= set(files):
        raise EvidenceError('primary archive lacks registered attempts or preregistration')
    with tarfile.open(archive_path, 'r:gz') as tf:
        members = {}
        all_files = set()
        for member in tf.getmembers():
            raw_name = member.name.rstrip('/')
            name = relative_name(raw_name) if raw_name else ''
            if not name:
                continue
            if member.isfile():
                if name in members:
                    raise EvidenceError('duplicate archive member: ' + name)
                members[name] = member
                all_files.add(name)
            elif member.isdir():
                if not any(path.startswith(name + '/') for path in files):
                    raise EvidenceError('unregistered archive directory: ' + name)
            else:
                raise EvidenceError('archive contains a link or special file: ' + name)
        if all_files != set(files):
            missing = sorted(set(files) - all_files)[:5]
            extra = sorted(all_files - set(files))[:5]
            raise EvidenceError(f'archive file inventory differs from manifest; missing={missing}, extra={extra}')
        attempts = parse_json(archive_bytes(tf, members, 'attempts.json', files), 'attempts.json')
        preregistration = parse_json(archive_bytes(tf, members, 'preregistration.json', files), 'preregistration.json')
    if not isinstance(attempts, list) or not isinstance(preregistration, dict) or not isinstance(preregistration.get('candidates'), list):
        raise EvidenceError('invalid attempt or candidate registration list')
    if any(not isinstance(row, dict) or not isinstance(row.get('id'), str) or not row['id'] for row in attempts):
        raise EvidenceError('invalid attempt identity')
    if any(not isinstance(row, dict) or not isinstance(row.get('id'), str) or not row['id'] for row in preregistration['candidates']):
        raise EvidenceError('invalid preregistered candidate identity')
    if len({row.get('id') for row in attempts}) != len(attempts) or len({row.get('id') for row in preregistration['candidates']}) != len(preregistration['candidates']):
        raise EvidenceError('duplicate attempt or registered candidate identity')
    return primary_root, manifest_path, manifest_bytes, manifest_sha, archive_path, archive_manifest, files, attempts, preregistration


def extract_registered_roots(scratch, archive_path, files, attempts):
    """Extract only verified candidates with retained matrices from the archive."""
    candidates = []
    for attempt in attempts:
        identity = attempt.get('id')
        if not isinstance(identity, str) or PurePosixPath(identity).parts != (identity,):
            raise EvidenceError('invalid historical candidate ID')
        if attempt.get('verified') is True and identity + '/matrix.json' in files:
            candidates.append(identity)
    selected = {'preregistration.json'}
    selected.update(path for path in files if any(path.startswith(identity + '/') for identity in candidates))
    scratch = Path(scratch).resolve()
    with tarfile.open(archive_path, 'r:gz') as tf:
        members = {member.name.rstrip('/'): member for member in tf.getmembers() if member.isfile()}
        for relative in sorted(selected):
            if relative not in files:
                if relative == 'preregistration.json':
                    raise EvidenceError('registered preregistration file is absent')
                continue
            member = members.get(relative)
            if member is None:
                raise EvidenceError('registered file is absent from archive: ' + relative)
            target = scratch / relative
            if not target.resolve().is_relative_to(scratch):
                raise EvidenceError('archive path leaves extraction directory')
            target.parent.mkdir(parents=True, exist_ok=True)
            expected = files[relative]
            if member.size != expected['bytes']:
                raise EvidenceError('archive member size changed: ' + relative)
            source = tf.extractfile(member)
            if source is None:
                raise EvidenceError('cannot open archive member: ' + relative)
            digest = hashlib.sha256()
            count = 0
            with open(target, 'xb') as destination:
                while True:
                    chunk = source.read(1024 * 1024)
                    if not chunk:
                        break
                    count += len(chunk)
                    if count > expected['bytes']:
                        raise EvidenceError('archive member exceeds registered size: ' + relative)
                    digest.update(chunk)
                    destination.write(chunk)
            if count != expected['bytes'] or digest.hexdigest() != expected['sha256']:
                raise EvidenceError('archive member digest changed: ' + relative)
    return candidates


def candidate_registration(preregistration):
    return {row['id']: row for row in preregistration['candidates']}


def validate_attempt_registration(attempts, registered):
    attempt_ids = [row.get('id') for row in attempts]
    if any(not isinstance(identity, str) or not identity for identity in attempt_ids):
        raise EvidenceError('invalid attempt identity')
    if len(attempt_ids) != len(set(attempt_ids)):
        raise EvidenceError('duplicate attempt identity')
    orphaned = sorted(set(attempt_ids) - registered.keys())
    if orphaned:
        raise EvidenceError('attempt identity is absent from preregistration: ' + ', '.join(orphaned[:5]))


def require_core_match(attempt, fault, registered):
    for field in CORE_FIELDS:
        if attempt.get(field) != fault.get(field) or attempt.get(field) != registered.get(field):
            raise EvidenceError('attempt, fault and registration differ for ' + field)
    for field in ['verified', 'killedBy', 'unaffected', 'recordDigests']:
        if attempt.get(field) != fault.get(field):
            raise EvidenceError('attempt and fault record differ for ' + field)
    if fault.get('verified') is not True or not isinstance(fault.get('killedBy'), list) or not fault['killedBy']:
        raise EvidenceError('fault is not a verified historical assertion regression')
    if len(set(fault['killedBy'])) != len(fault['killedBy']):
        raise EvidenceError('duplicate original detecting-test labels')


def validate_declared_prefix(matrix, plan, registered_limit):
    provenance = matrix.get('provenance', {})
    if provenance.get('predeclaredPrefixLimit') != registered_limit:
        raise EvidenceError('matrix prefix limit differs from preregistration')
    if plan.get('complete') is not True or not isinstance(plan.get('sites'), list):
        raise EvidenceError('registered planning output is incomplete or inconsistent')
    planned = len(plan['sites'])
    if provenance.get('plannedMutants') != planned:
        raise EvidenceError('planned mutant count differs from recorded planning output')
    expected_count = min(planned, registered_limit)
    mutants = matrix.get('mutants')
    if not isinstance(mutants, list) or len(mutants) != expected_count:
        found = len(mutants) if isinstance(mutants, list) else 'invalid'
        raise EvidenceError(f'incomplete declared mutant prefix: expected {expected_count}, found {found}')
    for index, (mutant, site) in enumerate(zip(mutants, plan['sites'][:expected_count])):
        if mutant.get('id') != site.get('id') or mutant.get('mutation') != site or mutant.get('evidence') != f'mutant-{index:04d}/execution.json':
            raise EvidenceError('matrix column differs from the registered planning prefix')
    return planned, expected_count, mutants


def validate_runtime_event(record, frozen_node, baseline_input, is_faulty, relative_path):
    if record.get('node') != frozen_node or record.get('expectedNode') != frozen_node:
        raise EvidenceError('runtime identity changed in a retained execution event: ' + relative_path)
    if not is_faulty and record.get('request', {}).get('inputDigest') != baseline_input:
        raise EvidenceError('runner or input identity changed in a retained execution event: ' + relative_path)


def validate_events(candidate_root, attempt, fault, registered, preregistration, files):
    candidate_root = Path(candidate_root).resolve(strict=True)
    require_core_match(attempt, fault, registered)
    record_digests = fault.get('recordDigests')
    if not isinstance(record_digests, dict) or not set(DIGEST_REQUIRED) <= set(record_digests):
        raise EvidenceError('fault record does not bind its matrix and fixed replay records')
    for relative, expected_sha in record_digests.items():
        relative = relative_name(relative)
        path = candidate_root / relative
        if not path.resolve().is_relative_to(candidate_root) or not path.is_file():
            raise EvidenceError('recorded evidence path is missing or leaves candidate root: ' + relative)
        actual = sha_file(path)
        if actual != expected_sha:
            raise EvidenceError('recorded evidence digest changed: ' + relative)
        archived = files.get(candidate_root.name + '/' + relative)
        if archived is None or archived['sha256'] != actual:
            raise EvidenceError('recorded evidence digest disagrees with archive manifest: ' + relative)
    matrix = read_json(candidate_root / 'matrix.json')
    recorded_analysis = read_json(candidate_root / 'analysis.json')
    computed_analysis = analyze(matrix)
    if computed_analysis != recorded_analysis:
        raise EvidenceError('primary analysis no longer matches its bound matrix')
    if matrix.get('subject', {}).get('faultId') != attempt['id'] or matrix.get('subject', {}).get('revision') != attempt['fix']:
        raise EvidenceError('matrix fault identity differs from attempt')
    if fault.get('rankingExcluded') and recorded_analysis.get('complete'):
        raise EvidenceError('primary exclusion label disagrees with the retained matrix')
    registered_limit = preregistration.get('mutantPrefixLimit')
    if type(registered_limit) is not int or registered_limit < 1:
        raise EvidenceError('preregistration does not provide a positive mutant prefix limit')
    provenance = matrix.get('provenance', {})
    plan = read_json(candidate_root / 'plan.json')
    plan_stdout = read_json(candidate_root / 'planning/stdout.txt')
    if plan != plan_stdout:
        raise EvidenceError('registered planning output is incomplete or inconsistent')
    planning_tree_sha = hash_tree(candidate_root / 'planning')[0]
    if planning_tree_sha != provenance.get('planSha256'):
        raise EvidenceError('planning output digest differs from the matrix')
    planned, expected_count, mutants = validate_declared_prefix(matrix, plan, registered_limit)
    plan_config = read_json(candidate_root / 'plan-config.json')
    selected_files = plan_config.get('selection', {}).get('include')
    if selected_files != attempt.get('sourceFiles'):
        raise EvidenceError('planning source selection differs from the fixed candidate registration')
    frozen_node = preregistration.get('node', {}).get(attempt['subject'])
    if not isinstance(frozen_node, dict) or fault.get('node') != frozen_node:
        raise EvidenceError('fault runtime identity differs from preregistration')
    if provenance.get('node') is not None and provenance['node'] != frozen_node:
        raise EvidenceError('matrix runtime identity differs from preregistration')
    baseline_path = candidate_root / ('fixed-before' if (candidate_root / 'fixed-before').is_dir() else 'baseline')
    if not baseline_path.is_dir():
        raise EvidenceError('fixed-before baseline directory is absent')
    executions = [baseline_path / 'execution.json', candidate_root / 'faulty/execution.json', candidate_root / 'fixed-after/execution.json']
    executions.extend(sorted(candidate_root.glob('mutant-*/execution.json')))
    if len(executions) != expected_count + 3:
        raise EvidenceError('one or more declared columns lack runtime evidence')
    baseline_execution = read_json(executions[0])
    baseline_input = baseline_execution.get('request', {}).get('inputDigest')
    if not baseline_input:
        raise EvidenceError('fixed-before runtime input identity is missing')
    event_digests = {}
    for event in executions:
        record = read_json(event)
        relative = event.relative_to(candidate_root).as_posix()
        validate_runtime_event(record, frozen_node, baseline_input, event == candidate_root / 'faulty/execution.json', relative)
        actual_sha = sha_file(event)
        archived = files.get(candidate_root.name + '/' + relative)
        if archived is None or archived['sha256'] != actual_sha:
            raise EvidenceError('execution event digest disagrees with archive manifest: ' + relative)
        event_digests[relative] = actual_sha
    for relative in ('plan.json', 'plan-config.json', 'planning/stdout.txt'):
        actual_sha = sha_file(candidate_root / relative)
        archived = files.get(candidate_root.name + '/' + relative)
        if archived is None or archived['sha256'] != actual_sha:
            raise EvidenceError('planning evidence digest disagrees with archive manifest: ' + relative)
    return {
        'matrix': matrix,
        'analysis': recorded_analysis,
        'frozenNode': frozen_node,
        'baselinePath': baseline_path.name,
        'plannedMutants': planned,
        'registeredPrefixLimit': registered_limit,
        'declaredPrefixCount': expected_count,
        'attemptedMutants': len(mutants),
        'planningTreeSha256': planning_tree_sha,
        'planSha256': sha_file(candidate_root / 'plan.json'),
        'planConfigSha256': sha_file(candidate_root / 'plan-config.json'),
        'eventDigests': dict(sorted(event_digests.items())),
        'recordDigests': dict(sorted(record_digests.items())),
        'faultLabelSha256': canonical_sha({'verified': fault['verified'], 'killedBy': fault['killedBy'], 'relatedGroup': fault.get('relatedGroup')}),
    }


def evaluation_groups(evaluations):
    return {row['fault']['id']: row for row in evaluations}


def cohort_counts(evaluations):
    groups = {row['fault'].get('relatedGroup', row['fault']['id']) for row in evaluations}
    return {'faults': len(evaluations), 'relatedGroups': len(groups)}


def verify_analysis_inputs(plan_path, plan_sha, manifest_path, manifest_sha, archive_path, archive_sha, module_digests):
    checks = [
        ('reviewed plan', plan_path, plan_sha),
        ('primary manifest', manifest_path, manifest_sha),
        ('primary archive', archive_path, archive_sha),
    ]
    for label, path, expected in checks:
        if sha_file(path) != expected:
            raise EvidenceError('analysis input changed during processing: ' + label)
    if analysis_module_digests() != module_digests:
        raise EvidenceError('analysis code changed during processing')


def run_secondary(plan_path, primary_root, output_dir):
    plan_path = Path(plan_path).resolve(strict=True)
    module_digests = analysis_module_digests()
    plan_bytes = plan_path.read_bytes()
    plan_digest = sha_bytes(plan_bytes)
    plan = validate_plan(parse_json(plan_bytes, plan_path), plan_digest)
    (
        primary_root,
        manifest_path,
        manifest_bytes,
        manifest_digest,
        archive_path,
        archive_manifest,
        files,
        attempts,
        preregistration,
    ) = load_archive_inputs(primary_root)
    registered = candidate_registration(preregistration)
    validate_attempt_registration(attempts, registered)
    output_path = Path(output_dir)
    if output_path.exists() or output_path.is_symlink():
        raise EvidenceError('secondary output path already exists')
    output_dir = output_path.resolve()
    output_dir.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix='test-value-secondary-') as scratch_name:
        scratch = Path(scratch_name)
        eligible_roots = extract_registered_roots(scratch, archive_path, files, attempts)
        candidate_results = []
        primary_evaluations = []
        secondary_evaluations = []
        paired_ids = set()
        for attempt in attempts:
            identity = attempt['id']
            result = {
                'id': identity,
                'subject': attempt.get('subject'),
                'relatedGroup': attempt.get('relatedGroup'),
                'verifiedPrimaryFault': attempt.get('verified') is True,
                'primaryFaultLabels': copy.deepcopy(attempt.get('killedBy', [])),
                'attemptRowSha256': canonical_sha(attempt),
            }
            if attempt.get('verified') is not True:
                result['secondaryExclusion'] = 'Primary fix replay was not verified; fault labels are not promoted.'
                result['primaryExclusion'] = 'Primary fix replay was not verified.'
                candidate_results.append(result)
                continue
            if identity not in eligible_roots:
                result['secondaryExclusion'] = 'Verified primary replay has no retained matrix and event root.'
                result['primaryExclusion'] = 'No retained primary mutation matrix.'
                candidate_results.append(result)
                continue
            if identity not in registered:
                raise EvidenceError('verified attempt is absent from the preregistered candidate list: ' + identity)
            candidate_root = scratch / identity
            fault_path = candidate_root / 'fault.json'
            fault = read_json(fault_path)
            binding = validate_events(candidate_root, attempt, fault, registered[identity], preregistration, files)
            original_matrix_bytes = (candidate_root / 'matrix.json').read_bytes()
            fault_labels_before = canonical_sha({'verified': fault['verified'], 'killedBy': fault['killedBy'], 'relatedGroup': fault.get('relatedGroup')})
            result.update({
                'faultRecordSha256': sha_file(fault_path),
                'matrixSha256': sha_bytes(original_matrix_bytes),
                'analysisSha256': sha_file(candidate_root / 'analysis.json'),
                'fixedBaselinePath': binding['baselinePath'],
                'runtime': binding['frozenNode'],
                'recordDigests': binding['recordDigests'],
                'plannedMutants': binding['plannedMutants'],
                'declaredPrefixLimit': binding['registeredPrefixLimit'],
                'attemptedMutants': binding['attemptedMutants'],
                'planningTreeSha256': binding['planningTreeSha256'],
                'planSha256': binding['planSha256'],
                'planConfigSha256': binding['planConfigSha256'],
                'eventDigests': binding['eventDigests'],
                'faultLabelSha256': binding['faultLabelSha256'],
                'primaryComplete': binding['analysis']['complete'] is True,
                'primaryRankingExclusion': fault.get('rankingExcluded'),
            })
            primary_eligible = binding['analysis']['complete'] is True and not fault.get('rankingExcluded') and bool(fault.get('killedBy'))
            if primary_eligible:
                evaluation = compact_evaluation(evaluate_fault(binding['matrix'], copy.deepcopy(fault), plan['comparisons']['seedCount']), binding['matrix'])
                primary_evaluations.append(evaluation)
                result['primaryEvaluation'] = 'primary-evaluations/' + identity + '.json'
            elif fault.get('rankingExcluded'):
                result['primaryExclusion'] = fault['rankingExcluded']
            else:
                result['primaryExclusion'] = 'Primary matrix is incomplete under the unchanged assertion-only policy.'
            try:
                derived_dir = output_dir / 'derived' / identity
                derive(candidate_root, derived_dir)
                derived_matrix = read_json(derived_dir / 'matrix.json')
                derived_analysis = read_json(derived_dir / 'analysis.json')
                if derived_matrix.get('subject') != binding['matrix'].get('subject'):
                    raise EvidenceError('derived matrix changed the original fault identity')
                labels_after = canonical_sha({'verified': fault['verified'], 'killedBy': fault['killedBy'], 'relatedGroup': fault.get('relatedGroup')})
                if labels_after != fault_labels_before:
                    raise EvidenceError('primary assertion fault labels changed during derivation')
                result['derivedMatrixSha256'] = sha_file(derived_dir / 'matrix.json')
                result['derivedAnalysisSha256'] = sha_file(derived_dir / 'analysis.json')
                result['derivedEventsPath'] = 'derived/' + identity + '/policy-and-events.json'
                result['derivedEventsSha256'] = sha_file(derived_dir / 'policy-and-events.json')
                result['derivedComplete'] = derived_analysis['complete'] is True
                if derived_analysis['complete'] is not True:
                    reasons = Counter(row['reason'] for row in derived_analysis.get('excluded', []))
                    result['secondaryExclusion'] = 'Derived matrix is incomplete: ' + json.dumps(dict(sorted(reasons.items())), sort_keys=True)
                elif not fault.get('killedBy'):
                    result['secondaryExclusion'] = 'Primary verified fault has no original detecting-test labels.'
                else:
                    evaluation = compact_evaluation(evaluate_fault(derived_matrix, copy.deepcopy(fault), plan['comparisons']['seedCount']), derived_matrix)
                    secondary_evaluations.append(evaluation)
                    result['secondaryEvaluation'] = 'secondary-evaluations/' + identity + '.json'
                    if primary_eligible:
                        paired_ids.add(identity)
                if (candidate_root / 'matrix.json').read_bytes() != original_matrix_bytes:
                    raise EvidenceError('primary matrix bytes changed during sensitivity derivation')
                if sha_file(fault_path) != result['faultRecordSha256']:
                    raise EvidenceError('primary fault record bytes changed during sensitivity derivation')
            except (OSError, ValueError, KeyError, TypeError) as error:
                if isinstance(error, EvidenceError):
                    raise
                result['secondaryExclusion'] = 'Derivation excluded this candidate: ' + str(error)
                result['derivedComplete'] = False
            candidate_results.append(result)
        primary_by_id = evaluation_groups(primary_evaluations)
        secondary_by_id = evaluation_groups(secondary_evaluations)
        paired_primary = [primary_by_id[identity] for identity in sorted(paired_ids)]
        paired_secondary = [secondary_by_id[identity] for identity in sorted(paired_ids)]
        added_secondary = [row for row in secondary_evaluations if row['fault']['id'] not in primary_by_id]
        aggregates = {
            'primary': summarize_faults(primary_evaluations),
            'secondary': summarize_faults(secondary_evaluations),
            'pairedPrimary': summarize_faults(paired_primary),
            'pairedSecondary': summarize_faults(paired_secondary),
            'addedSecondaryEligible': summarize_faults(added_secondary),
        }
        if 'aggregate.json' in files:
            with tarfile.open(archive_path, 'r:gz') as tf:
                members = {member.name.rstrip('/'): member for member in tf.getmembers() if member.isfile()}
                original_aggregate = parse_json(archive_bytes(tf, members, 'aggregate.json', files), 'aggregate.json')
            if aggregates['primary'] != original_aggregate:
                raise EvidenceError('recomputed primary aggregate differs from retained assertion-only aggregate')
        verify_analysis_inputs(
            plan_path,
            plan_digest,
            manifest_path,
            manifest_digest,
            archive_path,
            archive_manifest['archiveSha256'],
            module_digests,
        )
        for evaluation in primary_evaluations:
            write_json_safe(output_dir / 'primary-evaluations' / (evaluation['fault']['id'] + '.json'), evaluation)
        for evaluation in secondary_evaluations:
            write_json_safe(output_dir / 'secondary-evaluations' / (evaluation['fault']['id'] + '.json'), evaluation)
        write_json_safe(output_dir / 'aggregates.json', aggregates)
        result_manifest = {
            'schemaVersion': 1,
            'artifactKind': 'offline historical test-failure sensitivity analysis',
            'policy': POLICY['id'],
            'registeredPlan': {'path': str(plan_path), 'sha256': plan_digest, 'candidateManifestSha256': plan['candidateManifest']['sha256']},
            'analysisCodeSha256': module_digests,
            'primaryEvidence': {
                'root': str(primary_root),
                'manifestPath': str(manifest_path),
                'manifestSha256': manifest_digest,
                'archivePath': str(archive_path),
                'archiveSha256': archive_manifest['archiveSha256'],
                'attemptsSha256': files['attempts.json']['sha256'],
                'preregistrationSha256': files['preregistration.json']['sha256'],
                'registeredCandidateCount': len(registered),
                'attemptCount': len(attempts),
                'candidateIdsWithRetainedVerifiedMatrices': eligible_roots,
            },
            'summary': {
                'primaryAssertionEligible': cohort_counts(primary_evaluations),
                'secondaryEligible': cohort_counts(secondary_evaluations),
                'pairedEligible': cohort_counts(paired_primary),
                'additionalSecondaryEligible': cohort_counts(added_secondary),
                'primaryVerifiedAttempts': sum(row.get('verified') is True for row in attempts),
                'primaryUnverifiedAttemptsKeptOut': sum(row.get('verified') is not True for row in attempts),
            },
            'aggregatesPath': 'aggregates.json',
            'candidates': candidate_results,
            'interpretation': plan['interpretation'],
            'limits': 'A test-bound exception may add an observed kill under the separate secondary policy. It does not strengthen assertions, change primary fault labels, or validate a test-retention score.',
        }
        write_json_safe(output_dir / 'manifest.json', result_manifest)
        verify_analysis_inputs(
            plan_path,
            plan_digest,
            manifest_path,
            manifest_digest,
            archive_path,
            archive_manifest['archiveSha256'],
            module_digests,
        )
        return result_manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--primary-root', type=Path, required=True, help='Directory containing manifest.json and its registered archive')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = run_secondary(args.plan, args.primary_root, args.output)
    print(json.dumps(result['summary'], sort_keys=True))


if __name__ == '__main__':
    main()
