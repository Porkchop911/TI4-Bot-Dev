# Structured Diplomacy Adapter Boundary v1

The v1 engine has no natural-language or LLM runtime dependency. Future human table talk may use
`ti4_bridge::diplomacy::DiplomacyAdapter`, but the adapter is an untrusted translator with no
mutable `GameState` and no transition API.

The authority flow is:

`text -> DiplomacyIntent -> engine candidate generation -> current Choice -> CanonicalIntent -> engine window`

Only `canonicalize_selected_option(&Choice, option_id)` can produce `CanonicalIntent`. It checks
the ID against the current engine-generated legal options. Draft terms, parser payloads, aliases,
amounts, targets, and deadlines never cross this gate. The diplomacy window applies its stored
candidate rather than adapter-supplied data.

`Unsupported`, `Ambiguous`, and `Illegal` remain non-executable validation results. Renderers
receive only `DiplomacyPromptContext` and `StructuredDiplomacyResponse`; their prose cannot alter
the selected option. Bot-to-bot play bypasses this module entirely.

The serializable Rust contracts serve as the v1 schema fixtures. Empty, vague, malformed, and
oversized drafts are representable only as untrusted input and require future engine
canonicalization. No attempt is made to infer subjective promises such as “help”, “protect”, or
“be fair”.
