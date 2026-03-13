# Sync PR Review Rubric

## Purpose

This document is for reviewing `sync/upstream-*` PRs.  
It helps make a quick decision: `Approve`, `Request changes`, or `Escalate`.

The main goal is to verify not only that the merge builds technically, but also that the **trust, approval, and execution semantics boundaries of the fork have not drifted**.

Related documents:
- [`sync-strategy.md`](sync-strategy.md) — fork sync strategy
- [`delta-registry.md`](delta-registry.md) — fork delta registry
- [`ipc-plan.md`](ipc-plan.md) — IPC implementation plan

## 1. Approve

Mark `Approve` if all of the following are true at the same time:

- the PR does not touch `shared-hotspot` files
- or it touches them only superficially without semantic changes
- base CI is green
- fork invariants are green
- there are no changes in:
  - auth
  - trust resolution
  - approval routing
  - quarantine semantics
  - channel behavior
- `delta-registry.md` does not need an update
- the diff does not change the fork boundary

**Meaning**: upstream arrived safely and fork semantics were preserved.

## 2. Request Changes

Mark `Request changes` if any of the following applies:

- a conflict was resolved, but it is not clear how fork semantics were preserved
- a `shared-hotspot` was touched, but the review notes are empty
- CI is green, but fork invariants were not run
- config / gateway / tools hooks changed, and it is unclear whether IPC was broken
- a new fork-only piece appeared, but it was not recorded in `delta-registry.md`
- the PR is too large and does not highlight the risky areas

**Meaning**: the merge may be technically possible, but it is not yet proven architecturally.

## 3. Escalate

Treat the PR as an escalation case if it touches any of the following:

- `security/pairing.rs`
- gateway auth
- approval flow
- quarantine lane
- channel auto-approve behavior
- scheduler / execution semantics
- revoke / disable / downgrade semantics
- token metadata / identity model

Escalation is also required if:

- upstream changed the core assumptions the fork depends on
- the merge requires rethinking part of `delta-registry.md`
- there is a risk that a fork invariant still passes formally, but its meaning has changed

**Meaning**: this is no longer just review; it is an architectural decision.

## 4. Five Mandatory Questions

Before review, ask yourself:

1. Was the boundary between `upstream generic` and `fork-specific policy` preserved?
2. Was the trust / auth / approval path weakened?
3. Can quarantine or lateral messaging now bypass the old restrictions?
4. Has fork logic spread further across shared-hotspot files?
5. Is it time to move part of the diff upstream instead of continuing to grow the fork?

If the answer to even one question is “not sure”:

- do not mark `Approve`
- at minimum mark `Request changes`
- for security-sensitive areas, `Escalate` immediately

## 5. Quick Decision Table

| Situation | Decision |
|---|---|
| PR is clean, CI is green, hotspots are untouched | `Approve` |
| Hotspots are touched, but semantics are preserved and this is shown clearly | `Approve` or `Request changes` |
| Hotspots are touched, but preservation of fork semantics is not proven | `Request changes` |
| Auth / approval / quarantine / identity boundaries are touched | `Escalate` |
| There is doubt about whether the fork-only delta grew | `Request changes` |
| There is doubt about whether the security model was broken | `Escalate` |

## 6. Review Checklist

- [ ] upstream range is clear
- [ ] hotspot paths were reviewed
- [ ] fork invariants were run and are green
- [ ] `delta-registry.md` was updated if the fork boundary changed
- [ ] approval / quarantine / trust semantics were not weakened
- [ ] there is no hidden bypass through channels, scheduler, or tool routing

## 7. Practical Rule

- `Approve` — when the risk is understood and closed
- `Request changes` — when the evidence is insufficient
- `Escalate` — when the trust boundary, authority, or execution path was touched

Administrator must not look at a sync PR as “just another merge.”  
It is a review of whether **the security and product boundaries of the fork have drifted after the latest upstream integration**.
