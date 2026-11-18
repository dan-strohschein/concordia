# Contributing to Concordia

Thank you for helping. Contributions are welcome from humans and AI-assisted workflows alike, under the same rules.

## Ground rules
- **TDD:** write the tests first, commit them failing, then implement.
- **Engineering rules:** see `AGENTS.md`.
- **Clean room:** read `docs/clean-room.md`. Never contribute code copied, ported or translated from Unreal Engine or other non-open-source engines. Agents must not be given forbidden source.
- **Subscription-first:** no change may require an LLM API key.

## Workflow
```
just doctor        # check your toolchain
just test          # all tests
just lint          # fmt + clippy
just gate M<n>     # full milestone gate
```

## Developer Certificate of Origin
Every commit must be signed off (`git commit -s`), adding a line like:

```
Signed-off-by: Your Name <you@example.com>
```

By signing off you certify the Developer Certificate of Origin 1.1 (https://developercertificate.org):

> By making a contribution to this project, I certify that:
> (a) The contribution was created in whole or in part by me and I have the right to submit it under the open source license indicated in the file; or
> (b) The contribution is based upon previous work that, to the best of my knowledge, is covered under an appropriate open source license and I have the right under that license to submit that work with modifications, whether created in whole or in part by me, under the same open source license (unless I am permitted to submit under a different license), as indicated in the file; or
> (c) The contribution was provided directly to me by some other person who certified (a), (b) or (c) and I have not modified it.
> (d) I understand and agree that this project and the contribution are public and that a record of the contribution (including all personal information I submit with it, including my sign-off) is maintained indefinitely and may be redistributed consistent with this project or the open source license(s) involved.

## License
Concordia is licensed under either of Apache License 2.0 (`LICENSE-APACHE`) or MIT (`LICENSE-MIT`), at your option. Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion is dual-licensed as above, without any additional terms or conditions.
