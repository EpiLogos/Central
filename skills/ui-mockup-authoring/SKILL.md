---
name: ui-mockup-authoring
description: "Build a standalone HTML mockup of a product surface — screens, every state including failure and degraded, interaction, transitions, responsive layout, attention and motion — with each state traced to its Design, Vision and capability units. Use when a Design's surfaces must be seen and clicked before or alongside implementation. Not for production UI code or for writing the Design itself."
---

# UI mockup authoring

A Mockup renders what a Design describes so it can be looked at and operated.
It is a projection of Design, not a new source of meaning: when the Mockup
reveals a change, the change goes back to the Design.

## Form

Start from [assets/ui-mockup-template.html](assets/ui-mockup-template.html).
It uses the AIKit account template's colour tokens, has a state switcher, a
trace panel and hash routing (`#<state>`). It must stay standalone: no
external scripts or stylesheets; Google Fonts is the only permitted external
resource.

Each state is one `<section class="state" data-state="…">` carrying:

```text
data-design-ref      exact Design unit (file#heading-slug)
data-vision-ref      exact Vision unit (file#section-id)
data-capability-ref  capability id from the matrix
data-transition-to   the states this one can move to (space-separated)
```

## Rules

1. **All states.** Default, empty, loading, degraded and failure at least,
   wherever the Design says they can occur. Degraded and failure states say
   plainly what still works and how to recover.
2. **Real interaction.** Buttons move between states the way the Design says;
   transitions and motion respect `prefers-reduced-motion`.
3. **Responsive.** Check at phone width (about 375px) and desktop.
4. **Attention.** The first thing a person should notice is visually first.
5. **Trace every state.** No state without its Design, Vision and capability
   refs; an unknown ref is written as unresolved, not guessed.
6. **Standing.** A mockup is `agent-inference` until its Design is adopted.

## Verification

Open the file in a browser; step through every state with the switcher and the
in-state controls; check the trace panel shows the right refs; check phone
width and dark theme. `python3 tools/documentation_inventory.py <path>` reports
role `mockup` and every state id.

## Return

Changes discovered while mocking go back to the Design (and through it, if
needed, to Vision or Architecture).
