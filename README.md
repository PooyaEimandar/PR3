# PR3 — Portable Private Personal Intelligence

PR3 is an open-source, privacy-first personal AI runtime designed to work **offline on mobile and desktop**, or **cooperatively in a private hybrid** with a node the user controls. It is an embeddable intelligence engine, not a cloud chat wrapper: the same core is meant to run inside personal apps, OEM software, game engines and constrained devices.

## Where PR3 fits

PR3 is not another assistant. Platform owners ship on-device models and OS agent hooks for their own systems, and open-source assistants ship daemons with many channels. PR3 is the layer underneath: an embeddable runtime that decides what an agent may do, owned by the user and portable across Apple, Android, Windows and Linux.

- **Cross-device by design.** The phone confirms what the desktop or private node prepared, and memory follows the user across operating systems.
- **Measured on security.** The headline benchmark is authorization under prompt-injection attack ([AgentDojo](https://arxiv.org/abs/2406.13352)): with the same model, the target is no unauthorized actions and no data leaked through tool arguments.
- **Research lineage.** The closest prior work is [CaMeL](https://arxiv.org/abs/2503.18813) ("Defeating Prompt Injections by Design"). PR3 turns its propose/dispose idea into a portable runtime with device-bound approvals and a formally checked policy core.

## Design in brief

- **Voice first, text optional.** Talk to it, or send a voice note and read the reply; speech recognition and synthesis run on your devices. Over time it learns your style and routine choices, but never acts as you without your approval.
- **Local by default.** On-device inference uses the platform model where one exists (Apple Foundation Models, Gemini Nano) and a bundled GGUF model via llama.cpp where it does not. A user-owned private node takes long reasoning and always-on proactive work.
- **Runtime where Rust runs, protocol everywhere else.** The core is a sans-I/O, `no_std + alloc` Rust state machine. Rust apps use it directly through Cargo, other native hosts through a versioned C ABI and UniFFI bindings, and hosts that cannot run the runtime (consoles, Emscripten builds, MCUs) through a dependency-free C11 protocol client.
- **The model is not the authority.** Every model output is an untrusted proposal. Consequential actions go through structured intent, deterministic policy, user confirmation on a trusted device, a one-use capability and a hash-chained audit log. The model never sees a credential.
- **User-owned memory.** Memory is local, encrypted, inspectable, exportable and deletable, and stays usable without any model.

## Planned layout

```text
crates/pr3-core        sans-I/O core and trusted-side modules (intent, policy, capability, audit, identity)
crates/pr3-protocol    versioned wire contract
crates/pr3-inference   model-neutral inference traits
crates/pr3-llamacpp    bundled GGUF backend
crates/pr3-speech      on-device speech-to-text and text-to-speech
crates/pr3-ffi         C ABI and UniFFI bindings
client-c/              C11 protocol client for non-Rust hosts
apps/pr3-node          private inference and handoff node
```

The full workspace, crate modes and CI rules are in Roadmap.

## Roadmap
Gates, epics and risks are in [Roadmap.MD](Roadmap.MD).

## Attribution

PR3 is **inspired by [OpenJarvis](https://github.com/open-jarvis/OpenJarvis)**, in particular its split between intelligence, inference engines, agents, memory, evaluation and trace-driven learning. It is an independent implementation, not a fork, and is not endorsed by or affiliated with OpenJarvis or Stanford. 

## PR3?
**PR3** is more than just a name; it’s a tribute to my family. **P** stands for the first letter of my name, while **R3** represents the three most important R’s in my life: my wife, my son, and our dog, whose names all begin with R. A simple yet meaningful symbol of the family I cherish.

## License

[Apache-2.0](LICENSE).
