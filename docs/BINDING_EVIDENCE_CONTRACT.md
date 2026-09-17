# Binding evidence contract

## Scope

Preparation may consume a capability only when the producing path can justify
four independent relations:

```text
X ──identifies──────► A
A ──belongs to──────► S
S ──evaluated under─► P
S ──derived from────► I
```

`X`, `A`, `S`, `P`, and `I` are references to durable identities, not proof by
co-presence in a struct. The evidence may be produced by one authority or
composed from several authorities; this contract deliberately does not choose
that cardinality.

## Minimum evidence obligations

Each relation must expose:

- the exact subject and object identities;
- the version/schema of the record asserting the relation;
- the producer authority and its trust boundary;
- an independently verifiable integrity reference where bytes or history are
  relevant;
- failure behavior when either endpoint is absent, stale, or mismatched.

## Composition rules

- Composition must preserve endpoint identity; a valid edge for `A1` cannot be
  substituted for `A2` merely because both are `ActionOption` values.
- The final preparation capability must retain enough references to audit every
  edge without trusting generated prose.
- Adding one edge cannot silently imply any of the other three.
- A missing edge fails closed; it cannot be represented as an unverified
  `VerifiedPreparationSnapshot`.

## Non-goals

- No decision yet about one receipt versus multiple witnesses.
- No claim that a digest proves semantic ownership or legal correctness.
- No evaluator minting until its inputs cover the required relations.
