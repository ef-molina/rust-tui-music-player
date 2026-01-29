## Summary

Briefly describe what this PR changes and **why** it exists.

What problem does this solve?

---

## Architectural Alignment

Confirm that this change follows the project’s architectural rules:

- [ ] All state mutations occur in the main event loop
- [ ] No UI code performs I/O or mutates state
- [ ] No logging to stdout/stderr (tracing → debug.log only)
- [ ] Background work communicates via channels
- [ ] No long-running I/O added to the main thread

If any box cannot be checked, explain why below.

---

## Behavior Changes

Describe any user-visible or behavioral changes.

- New features:
- Modified behavior:
- Removed behavior:

If there are **no behavior changes**, state that explicitly.

---

## Files / Modules Touched

List the primary files or modules modified:

- `src/...`
- `src/...`

Explain _why_ these areas were changed.

---

## Manual Testing Performed

Check all that apply:

- [ ] Navigation responsiveness verified
- [ ] Playback works as expected
- [ ] Lyrics load / fail gracefully
- [ ] No terminal corruption (no stdout/stderr logging)
- [ ] mpv process exits cleanly on quit
- [ ] Relevant log output verified in `debug.log`

Describe any additional testing performed.

---

## Known Limitations / Follow-Ups

Are there any edge cases, technical debt, or follow-up work left by this PR?

---

## Screenshots / Logs (Optional)

If relevant, include screenshots or excerpts from `debug.log`.

---

## Final Checklist

- [ ] `cargo check` passes
- [ ] `cargo build --release` passes
- [ ] Code matches existing style and conventions
- [ ] Documentation updated if behavior changed
- [ ] No unrelated changes included

---

Thank you for contributing to **rust-tui-music-player**!
