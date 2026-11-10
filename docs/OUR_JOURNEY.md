# For the Empire: The Story of Ounce-Core

*An open letter from the founder — November 2026*

There is a kind of idea that won't leave you alone.

Ounce-Core began with one of those ideas: **what if we could build a new emulator?** More specifically, the original ambition was a Nintendo Switch 2 emulator. It was exciting, perhaps overly ambitious, and wrapped up in the kind of possibility that makes you open a terminal long after you should have gone to bed.

I didn't begin with a polished research plan. I began with curiosity, determination, and more questions than answers.

And that became a journey.

## The dream, and the difficult part

The early project moved quickly. AI coding assistants helped make that possible. They also made it easy to produce code, features and confident-sounding explanations faster than we could independently verify them.

Some of our original framing and claims ran ahead of the evidence. People noticed. There were critical questions about what the project could really do, how it had been built, and where parts of its history and source material came from.

Those questions were not comfortable. But discomfort is not the same thing as an unfair question.

A public software project needs more than enthusiasm. It needs provenance that can be examined, boundaries that can be understood, tests that somebody else can repeat, and the humility to say **we haven't proved that yet**.

Those concerns did not disappear because we changed direction. They are part of the record, and we continue to take them seriously. Readers should consult the project's [provenance](PROVENANCE.md), [clean-room rules](CLEAN_ROOM.md), and [evidence limits](LIMITS.md) rather than rely on this personal account for technical or legal assurance.

## The moment the idea changed

The most useful question turned out not to be, *Can we say we've built an emulator?*

It was: **What can this code actually help someone understand?**

As we tested real, permitted open-source workloads, we found a more interesting answer. Comparing execution against reference results could expose errors hiding inside an ARM64 runtime: instructions decoded incorrectly, floating-point operations behaving differently, missing vector operations, and failures that needed smaller, more precise explanations.

Ounce-Core was becoming useful as an **execution and verification research lab**.

That is the identity we have chosen to build around: understand why programs fail, and record when their behaviour agrees with a reference on a clearly defined test.

It was not a simple rebrand. It meant learning to trade exciting general claims for narrower, reproducible ones.

## A lot of code is not the same as a lot of proof

We built CPU execution paths, differential harnesses, a browser playground, debugging tools, test reports, documentation and automated checks. We added real open-source programs to bounded tests. We worked through difficult failures and celebrated when something finally matched.

There was a lot of laughter along the way, too. GitHub workflows broke. Agents did things that seemed helpful until they created three new problems. We devised grand plans and then discovered that the best improvement was sometimes to remove a rule, simplify a document, or stop adding machinery.

We called our little operation **“the empire.”** It was a joke, a morale booster and, on the difficult days, a reminder that building something takes persistence.

But here is the distinction that matters most:

**A passing test is not universal compatibility. A green CI badge is not independent reproduction. A hundred commits are not a hundred users.**

Our evidence is bounded by the exact programs, inputs, environments and comparisons recorded. Some important questions about coverage, historical provenance and independent reproduction remain open. That's why the project maintains [specific limits](LIMITS.md), [reproduction instructions](REPRODUCE.md), and invitations for outside people to challenge the results.

## The question that changed everything again

One evening, after all the roadmaps, PRs, agents and status checks, I asked a different question:

**“Are we working too hard?”**

We were getting better at producing work. We were not yet equally good at turning that work into value for someone outside the project.

So we made another choice.

For thirty days, from 10 November to 10 December 2026, we would consolidate. Fewer speculative features. Fewer management rituals. More care for reliability, safety, real problems, independent testers and the people trying the software for the first time.

Our next milestone isn't the longest changelog. It's one independent developer who can use Ounce-Core to investigate a real ARM64 problem, reproduce a result, and tell us what did or did not work.

That person might find a bug. They might challenge one of our assumptions. They might teach us something we missed.

All of those outcomes would be progress.

## What comes next

If you are a developer, researcher, student or simply a curious person who has ever asked *why did this program behave differently?*, we would love your careful scrutiny.

Start with the [reproduction guide](REPRODUCE.md), examine the [limits](LIMITS.md), try a bounded [real-program test](ADD_A_PROGRAM.md), or join the [independent verification discussion](https://github.com/hffgzktbcc-blip/ounce-core/issues/248). You don't have to arrive as a fan. A well-documented failure is as welcome as a pass.

And if you were here when this project had a different dream, thank you for caring enough to ask hard questions.

We still have a long way to go. There may never be a neat moment when everything is finished and every uncertainty is resolved.

But an idea that began as an emulator dream became an opportunity to learn how to build software more responsibly: by testing, admitting limits, listening to criticism, and leaving room for other people to verify what we've done.

**We had an idea. We refused to leave it as an idea. Now we have to make it useful to someone else.**

For the empire. 🫡

— *The Ounce-Core founder*

---

*This is a personal project history, not a capability claim, external validation, legal conclusion, or complete provenance account. For the current technical scope and evidence, the linked project documents take precedence.*
