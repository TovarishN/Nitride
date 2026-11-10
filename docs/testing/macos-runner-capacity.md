# macOS CI cancellation investigation

Checked November 10, 2026 against GitHub Actions job metadata and check-run
annotations. The two reported `main` failures after PRs #52 and #53 were hosted
runner-acquisition failures. No repository build or test command ran in either
cancelled macOS job.

## Evidence

Times below use Europe/Tallinn (EET). The retained
[API snapshot](2026-11-10-macos-runner-capacity.json) keeps the original UTC timestamps,
source revisions, job IDs and annotations.

| Checkpoint | macOS job | Started / completed | Result |
|---|---|---|---|
| PR #52 merge, `be203f9a` | [129313449818](https://github.com/TovarishN/Nitride/actions/runs/42661353000/job/129313449818) | Nov 9, 19:06:28–19:21:29 | Cancelled before runner assignment; no steps |
| PR #53 merge, `9f591977` | [129386353301](https://github.com/TovarishN/Nitride/actions/runs/42684156493/job/129386353301) | Nov 9, 21:31:31–21:46:32 | Cancelled before runner assignment; no steps |
| PR #54 merge, `f240c666` | [129840021771](https://github.com/TovarishN/Nitride/actions/runs/42819194698/job/129840021771) | Nov 10, 14:50:39–14:56:54 | Passed |

Both failed jobs have `runner_id: 0`, an empty runner name, zero steps, and this
GitHub failure annotation:

> The job was not acquired by Runner of type hosted even after multiple attempts

GitHub also attached repeated notices about capacity constraints affecting hosted
macOS arm64 availability. The apparent fifteen-minute duration was 15 minutes and
one second in each job's metadata, with no checkout or test execution. The other
platforms and quality/release-tool jobs passed in both runs.

The latest inspected `main` run passed all jobs. Its macOS job successfully ran
workspace tests, native-only CLI/process/LSP checks and LSP release-training
verification. This later success covers `f240c666`, not the subsequent local
cleanup, embedding, panic-audit or host-binding commits.

## Resolution

No source defect or test timeout was demonstrated. The annotations identify
runner provisioning, so this investigation does not change workflow timeouts or
runner labels. Workflow-level concurrency still cancels superseded work by
branch or pull request, but that was not the reported failure: there was no
replacement `main` run during either fifteen-minute interval, and GitHub explicitly
reported acquisition failure.

If the same annotation recurs, a failed-job rerun after hosted capacity recovers
is the appropriate operational response. There is no need to rerun the old
checkpoints now: the later `main` checkpoint already passed the same workflow.
This investigation did not rerun, cancel or modify any GitHub workflow.
