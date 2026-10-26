# PR3 - Portable Private Personal Intelligence

PR3 is an open-source, privacy-first personal AI runtime that works **offline on mobile and desktop**, or **cooperatively in a private hybrid** with a computer the user owns. It is not another assistant but the layer underneath one: an embeddable runtime that decides what an agent may do, owned by the user and portable across Apple, Android, Windows and Linux.

- **Local by default.** The model, speech recognition and voices run on your devices; a paired computer can lend a phone its larger model, and the phone answers on its own when it cannot reach it.
- **The model proposes, PR3 decides.** Every model output is an untrusted proposal. Consequential actions go through a structured intent, a deterministic policy, your approval with Face ID, Touch ID or a fingerprint, a one-use capability and a hash-chained audit log.
- **Memory you own.** Local, inspectable and deletable, with the provenance of every fact.
- **Measured on security.** On [AgentDojo](https://arxiv.org/abs/2406.13352), with the same model, no prompt-injection attack got through PR3 where a plain tool-calling loop let them through ([results](doc/benchmarks.md#86-results-so-far)).

## Quick start

```bash
script/fetch-model.sh               # the models config/pr3.toml selects
cargo test --workspace
cd app/pr3-app && cargo tauri dev   # the macOS app
```

iOS, Android, configuration, pairing and benchmarks: [doc/development.md](doc/development.md).

## Documentation

- [Overview](doc/overview.md): what PR3 is, its positioning and principles.
- [Architecture](doc/architecture.md) and [security](doc/security.md): how it is built and what it defends against.
- [Benchmarking](doc/benchmarks.md): what PR3 is measured on, and the results.
- [Roadmap](doc/roadmap.md): where the work stands and what comes next.
- [Everything else](doc/README.md): deployment, applications, quality gates, risks, decisions and the change log.

## Attribution

PR3 is **inspired by [OpenJarvis](https://github.com/open-jarvis/OpenJarvis)**, in particular its split between intelligence, inference engines, agents, memory, evaluation and trace-driven learning. It is an independent implementation, not a fork, and is not endorsed by or affiliated with OpenJarvis or Stanford.

## PR3?

**PR3** is more than just a name; it’s a tribute to my family. **P** stands for the first letter of my name, while **R3** represents the three most important R’s in my life: my wife, my son, and our dog, whose names all begin with R. A simple yet meaningful symbol of the family I cherish.

## License

[Apache-2.0](LICENSE).
