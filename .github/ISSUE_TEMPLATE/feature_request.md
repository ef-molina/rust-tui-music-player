---
name: Feature Request
about: Propose a new feature or enhancement
title: "[Feature] "
labels: enhancement
assignees: ""
---

## Problem Statement

What problem does this feature solve?

Why is the current behavior insufficient?

---

## Proposed Solution

Describe the feature you are proposing.

Focus on **behavior**, not implementation details.

---

## Architectural Fit

How does this feature fit within the existing architecture?

Confirm (or discuss):

- [ ] State would still live in `AppState`
- [ ] Behavior would be driven by `AppEvent`s
- [ ] UI remains pure (no side effects)
- [ ] No blocking I/O added to the main thread

If any box cannot be checked, explain why.

---

## Alternatives Considered

Are there simpler or existing ways to solve this problem?

Why is this approach preferred?

---

## Scope & Impact

Which areas of the codebase would this affect?

- UI
- Event loop
- Player
- Lyrics system
- Filesystem
- Metadata

---

## Non-Goals

Explicitly list what this feature will **not** attempt to do.

---

## Additional Context

Mockups, examples, or references (optional).

```

---

```
