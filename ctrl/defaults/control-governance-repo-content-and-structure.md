# Repo content and repo structure

**Status:** distributed default — becomes human source on adoption

A repository carries standing guidance about itself. This is the guardianship layer of the repo covenant: what a repository expects of anyone, human or agent, who works it.

The guidance separates into two classes, and the separation matters:

```text
repo structure
    how the repository is organised
    layout, placement, naming — what belongs where

repo content
    how the artefacts are made
    documentation conventions, coding styles, rules the work must satisfy
```

Structure answers *where does this go*. Content answers *what should this be like*. They drift for different reasons — structure erodes when things are added without a place, content erodes when style is renegotiated one commit at a time — and they are consulted at different moments: structure when something is added, content when something is made. A source that mixes them makes both harder to find and harder to revise without collateral damage.

## What is expected

Repos carry this guidance at their top level, openly. Encouragement, not tolerance.

- Top-level guidance is ordinary native source. A README, STANDARDS, CONTRIBUTING or AGENTS file that genuinely plays the role is recognised where it lives; it is not required to move.
- Prose first. Structure in a convention document must earn its maintenance cost. No universal schema, no mandated filenames — a filename is a discovery hint, never the authority.
- Rules are checkable where checking is possible, and honest where it is not.
- The two classes stay distinguishable: separate sources once both have weight; one source only while neither outgrows the other.

## Where it lives

The relation recurs at every scope, because ProjectCentral repeats Control:

```text
Control/agents/governance/repos/        the cross-project stance
ProjectCentral/agents/governance/       each repo's actual conventions
    repo-structure.md                   where things go in this repo
    repo-content.md                     what this repo's artefacts should be like
```

Project facts stay with their project. A convention learned in one repo is not promoted to the cross-project scope merely because it was useful once. What graduates across repos is the class of expectation, not the instance.

Native instruction files — `AGENTS.md`, `CLAUDE.md` and kin — may carry either class. Their standing is recognised, not assumed: adoption is explicit, and a generated suggestion stays generated until adopted.

## How it reaches the agents

The guidance is source, not prompt:

```text
authored guidance
        ↓
AIKit composition — bounded, accounted-for guidance capsules
        ↓
operative context of the situated agent
        ↓
return — wiki knowledge with provenance; proposals back to source
```

The wiki is implicated directly. Agent knowledge about a repo compiles against these sources and keeps their provenance, so drift between the stated convention and the lived repository becomes discoverable instead of silent.

## Maintenance

A proven project convention moves by proposal: instance to instance stays project-local; a genuine cross-project expectation arrives at the cross-project scope as an explicitly adopted statement. Guidance that no longer changes behaviour is pruned — a rule whose removal changes nothing was context cost, not care.
