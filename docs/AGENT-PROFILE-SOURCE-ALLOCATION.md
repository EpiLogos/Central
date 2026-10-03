---
role: architecture
standing: agent-inference
scope: Central
provenance: agent-proposed
---

# Agent/Profile source allocation

Standing: agent-inference. This account recovers existing source behavior at
Central `5e4510a6bd61d6e151c84d755f884b61b693db27`; it does not recognise a
new Agent, World or authority relation.

[Central #108](https://github.com/EpiLogos/Central/issues/108) and
[#112](https://github.com/EpiLogos/Central/issues/112) concern durable saved
configuration for an existing semantic AgentRef. A profile reference is not
an Agent identity, and saved defaults are not live Agency. Retain that account.

[PR #178](https://github.com/EpiLogos/Central/pull/178), merged as
`39efa03ea8607bdd8a79b0e317457c2ccc3ead4c`, adds a distinct source-proposal
allocator. Invoke the registered `agent-profile.express` Action through
`ctrl action run`; supply explicit World and nonempty ratified-World refs,
nonempty intent, and only the optional fields permitted by its contract.
The caller does not manufacture the Agent/Profile proposal refs or initial
revision. Central allocates them, preserves intent and referenced defaults,
and enters the existing proposal owner with a fixed expression origin.

The returned profile is `generated-proposal`, `unrecognised`, at `r1`, with
allocated refs. A ratified-World reference is supplied context, not World
promotion. The receipt does not establish reusable-definition Recognition,
AgentSet membership, Agency, AgentSession or execution/material authority.
Review, exact-source acceptance and runtime resolution retain their separate
native contracts. Source acceptance is not execution permission.

The store owns source persistence, scoped CAS and its actual proposal reading.
Actuation owns Agency and authority; AIKit owns consuming runtime resolution;
Workcell owns material hosting. This boundary keeps a recoverable source
expression from silently becoming permission to act.

Read [agent_profile_actions.rs](../ctrl/src/agent_profile_actions.rs),
[agent_profile_store.rs](../ctrl/src/agent_profile_store.rs) and
[agent_profile.rs](../ctrl/src/agent_profile.rs). Public owner tests in the
Action module exercise allocation and unrecognised-proposal roundtrip; the
[acceptance tests](../ctrl/tests/agent_profile_acceptance.rs) retain the
source-revision and no-execution-authority boundary. The
[workspace verification](https://github.com/EpiLogos/Central/actions/runs/36987855257)
passed at frozen head `eb1cb49213e86661db39c128df7fad322297d3b8`; its tree equals
the named merge cut. This qualifies source and hosted behavior, not personal
installation or composed desktop acceptance.

Open question: which separately recognised reusable Agent definition, Agency
and human experience will consume a particular proposal? The allocator result
does not decide that relation.
