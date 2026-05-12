# Feature: [Name]

*Status: Draft | In Review | Accepted*

## Goal

One sentence: what does this feature accomplish and why does it matter for the clock or the testing pyramid?

## Context

What problem are we solving?
What constraints apply (embedded, no_std, ESP32-C6, Wokwi-simulatable)?

## Decisions

- **Decision 1:** ...
- **Decision 2:** ...

## Open Questions

- [ ] Question one?
- [ ] Question two?

## Validation

Which testing tiers verify this feature, and how?

- [ ] Host tests (`just test`) — pure logic in `clock-pure`
- [ ] Wokwi simulation (`just act-ci`) — firmware behaviour without hardware
- [ ] Hardware-in-the-loop — physical device required
- [ ] Docs / config only — no automated verification needed

## Out of Scope

What this feature explicitly does not cover.
