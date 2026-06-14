# Novel Writing Quickstart

Run a novel with Nexus from a clean install — local-only, no platform account.

> **The canonical CLI surface lives in [`creator-run-preset-entry.md`](.mstar/knowledge/specs/creator-run-preset-entry.md) (Shipped Master V1.45).** This quickstart gives you the **happy path**; for the full three-plane IA, every flag, every preset id, see the spec.

## 1. Install + start

```bash
nexus42 system doctor              # runtime health
nexus42 creator register --name "Your Name"
nexus42 creator use <handle>
nexus42 creator workspace init
nexus42 daemon start               # keep running in another terminal
```

## 2. Start a novel

```bash
nexus42 creator world create --title "Neon River"   # get wld_… back
nexus42 creator bootstrap --idea "A solpac noir detective story in a floating canal city"
# → creates a Work, runs the init preset, chains intake → produce
```

## 3. Watch it run

```bash
nexus42 creator works status         # chapters, findings, next action
```

The daemon auto-advances through chapters by default. Inject direction any time:

```bash
nexus42 creator works inspire <work_id> --note "the partner is the informant"
```

## 4. Review findings

```bash
nexus42 creator run novel-review-master <work_id>         # enqueue master decision
nexus42 creator works status                                 # list open findings
nexus42 creator run reflection-loop <work_id>               # generate new findings
```

## 5. Done?

```bash
nexus42 creator works status           # COMPLETED + 12/12 finalized
nexus42 creator works completion-lock release <work_id>   # if you want to add more
nexus42 creator works reopen <work_id> --reason "epilogue"
```

## Where to look next

| You want... | Read this |
|---|---|
| Every command, flag, preset id | [`.mstar/knowledge/specs/creator-run-preset-entry.md`](.mstar/knowledge/specs/creator-run-preset-entry.md) |
| Repo layout, storage, ACP setup | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| How Works / chapters / volumes fit together | [`.mstar/knowledge/specs/novel-workflow-profile.md`](.mstar/knowledge/specs/novel-workflow-profile.md) |
| Build from source, contribute | [`CONTRIBUTING.md`](CONTRIBUTING.md) |

> Commands and surface will keep moving as V1.46+ lands. **Trust the spec, not this file.** If a command here disagrees with the spec, the spec is the source of truth.
