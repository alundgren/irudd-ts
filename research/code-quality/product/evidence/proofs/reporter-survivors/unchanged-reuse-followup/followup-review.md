The unchanged-input retry is complete. It ran the original five-test baseline fresh, reused 151 completed results and executed only the three initial timed-out sites. It records 66 kills, 88 survivors, no timeouts or execution errors, and no omitted results.

The input digest remains `6495a9264b85aac1dfbd67b59d482f804d76b12ff3745fac7a4da1335b63a1c3`. The new report SHA256 is `854772d2a52d28f5192c3e6321e009182f845a89b9992ae3063d2c2ba61c0d15`. [reuse-identity-validation.json](reuse-identity-validation.json) verifies the binary/configuration hashes, all 14 preflight inputs, unchanged original record bytes and unchanged reused evidence.

All 88 survivors are classified in [survivor-classification.followup-88.json](survivor-classification.followup-88.json). It retains all 85 initial classifications and adds these three useful missing public controls. Totals are 46 useful public controls, 40 implementation details, 2 equivalences qualified by documented one-shot source reasoning, and no unresolved survivor classifications.

| New survivor ID | Line | Changed behavior | Proposed public control |
| --- | ---: | --- | --- |
| `e0566803ff9f8d836d7e0e79e5d97d5bd0ac4b4bc8d4262f6ef1436b8ea45c47` | 94 | The API guard accepts a missing logger.error method when getUnhandledErrors exists. The bridge then hides the missing method until logging occurs, so an otherwise passing run can incorrectly finish complete. | A context with logger.error missing stays incomplete. Correcting it to a function permits a clean complete run. An existing logger called outside cleanup remains a negative control. |
| `1d6e7eaf63a9e823cd96dae222e8de382646f9a5a764e2d09848ce75bd3b01a8` | 129 | After the hook event budget is exhausted, additional hooks are dropped without marking the run unsupported. A passing oversized hook inventory can remain complete. | A finite hook inventory beyond the reporter event budget stays incomplete. A normal passing hook inventory completes without hook failure evidence. |
| `c06a6f4b79b9927050c983be8ce64421af72501165bc032ba5879339c303d417` | 136 | A second onTestRunEnd call is accepted when the module count remains within budget, so a repeated one-shot run can remain complete. | Ending the run twice stays incomplete. The corrected single end callback completes cleanly. The final V3 repeated-run control already fails this edit in a separate bounded Node proof. |

[behavior-probes.json](behavior-probes.json) retains the completed bounded Node witnesses. Each unchanged reporter stays incomplete on the negative input; the edited copy claims complete. These probes were completed after the installed-framework reuse run and introduce no production edit.

The original report remains incomplete with three timeouts, and the original 85-survivor review, correction provenance and raw proof receipts remain unchanged. The retry shows these three sites finish with unchanged inputs. It does not establish why their initial installed-framework commands reached the deadline. No timing comparison or mutation score target is claimed.

[reuse-identity-validation.json](reuse-identity-validation.json) links all six driver receipts saved in the repository ignored evidence directory.
