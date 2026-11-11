# Permission request — draft, not sent

> **Do not send.** On 8 November 2026 the owner decided to remove the program and not seek a grant (see [docs/PROVENANCE.md](../PROVENANCE.md), Owner items). This draft is kept only as a record. Send it only if the owner reverses that decision.

Recipient: the author/rightsholder of `pepsipu/ns2jb` on GitLab.
Subject: Permission for one historical 72-byte test function in Ounce-Core

Hello,

We are resolving a historical third-party input in Ounce-Core. Its old
`src/main.rs` attributed a 72-byte `NS2JB_TEST_FUNCTION_BIN` array to your
`engine/switch2/process/test-function.bin`. The copied bytes have SHA-256
`71544d5064344d40567311b57c6fd4d726c23346d02eec0bba4c210591401b42`.

That input was removed from current code in commit
`76ce7662a761de5c177e74e7d5c9b676a7c46041`, but the older source remains in Git
history. Our project MIT licence does not grant rights to your contribution.
We have not located a redistribution grant for this specific input.

If you hold the applicable rights, would you permit redistribution of this
specific test function in our historical repository revisions, with attribution,
under MIT or a separate written permission? An existing licence or notice that
already covers it would also resolve the question; please point us to it.

This request is limited to that test function. It is not a request for rights to
firmware, the rest of ns2jb, other contributors' material, or proprietary assets.
If additional rightsholders or conditions apply, please identify them. If you
cannot grant permission, we will retain that as an unresolved permission case
and evaluate a precisely scoped removal.

Thank you.

---

Owner review is required before sending. No request was sent or permission
received as part of this draft. Preserve any response verbatim with its source
and date; do not infer rights for other inputs from a response to this one.

Identification evidence: the source attribution and exact named array in
`76ce766^:src/main.rs`; the later object-level report independently records a
historical 72-byte outside-program case. This session re-hashed the embedded
array, not a newly downloaded upstream binary. No copied bytes are included here.

A read-only scan of 159 available local refs found that exact named array in
20 source blob objects, reachable from 157 of those refs, including the local
view of origin/main and tags v0.3.0 through v0.5.0. This is a dated local scope,
not every server ref, cache, fork or clone. Main/tag **current trees** do not
gain that program merely because its older source remains reachable. A history
rewrite would affect identities beyond the copied array and requires a separate
concrete migration plan; none was performed here.
