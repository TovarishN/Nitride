#!/usr/bin/env python3
"""Find complete rehearsal artifacts for the exact commit being tagged."""
import json
import os
from pathlib import Path
import subprocess


def eligible_runs(runs, sha, current_id):
    return sorted((int(run['id']) for run in runs
                   if run['head_sha'] == sha and int(run['id']) != current_id
                   and run['conclusion'] == 'success' and run['event'] == 'push'
                   and (run.get('head_branch') or '').startswith('codex/release-')), reverse=True)


def complete_artifacts(artifacts, wanted):
    native = [artifact for artifact in artifacts if artifact['name'].startswith('native-')]
    return (len(native) == len(wanted) and {item['name'] for item in native} == wanted
            and all(not item['expired'] for item in native))


def api(path):
    return json.loads(subprocess.check_output(['gh', 'api', path], text=True))


def main():
    found = ''
    if os.environ.get('GITHUB_REF_TYPE') == 'tag':
        repository = os.environ['GITHUB_REPOSITORY']
        sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
        current_id = int(os.environ['GITHUB_RUN_ID'])
        matrix = json.loads(Path(__file__).with_name('matrix.json').read_text())['include']
        wanted = {'native-' + row['id'] for row in matrix}
        # Reuse is optional; an API error falls back to a fresh complete build.
        try:
            runs = api(f'repos/{repository}/actions/workflows/release.yml/runs?head_sha={sha}&status=success&event=push&per_page=100')['workflow_runs']
            for run_id in eligible_runs(runs, sha, current_id):
                artifacts = api(f'repos/{repository}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
                if complete_artifacts(artifacts, wanted):
                    found = str(run_id)
                    break
        except subprocess.CalledProcessError:
            pass
    print(f'run_id={found}')


if __name__ == '__main__':
    main()
