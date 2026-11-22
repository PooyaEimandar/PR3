# Protocol and private hybrid

How a phone borrows its owner's computer's model, how the two pair, and how small boards reach a node. The phone keeps the conversation, memory, tools, approvals and keys; the node can only generate text and propose tool calls. The wire format is versioned and fixed byte for byte, so a phone and a node on different PR3 versions keep working together within one protocol version ([DATA-08](17-delivery-plan.md)). Boards with only a serial port use a separate accessory protocol through a gateway.

## Wire protocol v0

Version `0`; the largest message either side accepts is 1 MiB after decryption.

| Direction | Message (`type`) | Fields |
| --- | --- | --- |
| Phone → node | `hello` | `version`, `device` |
| | `generate` | `id`, `request` (an inference request: `id`, `session`, `messages`, `tools`) |
| | `cancel` | `id` |
| | `continuity` | `request` (`version`, `cursor`, `page.changes[]`) |
| Node → phone | `hello` | `version`, `node`, `model` |
| | `token` | `id`, `text` |
| | `done` | `id`, `generated` (`content`, `tool_calls`, `info`) |
| | `failed` | `id`, `reason` |
| | `continuity` | `reply` (`acknowledged`, `page`) |

A continuity change is `{sequence, id, kind, owner, epoch, revision, payload}` with a null payload as a permanent deletion. No approval, capability, vault key or execution request ever crosses this protocol.

### CBOR, byte for byte

`src/cbor/` is a bounded codec that needs no allocation outside the caller's arena ([D-09](19-decisions-and-risks.md)).

- **Encoding** is canonical: every message is a definite-length map with text keys in the order the table above lists them, the `type` key first; absent optional fields are omitted (`tool_calls` when empty, `tool_call_id` when absent); integers in their shortest major-type-0 or major-type-1 form; strings as definite-length text; a missing value as `null`; identifiers and timestamps as plain integers; tuples as arrays.
- **Decoding** is liberal within bounds: any integer width, definite or indefinite lengths, fields in any order, unknown fields skipped; depth at most 32, at most 4096 items per container, every length checked against the remaining input before it is trusted. Text must be valid UTF-8.
- Golden frames under `test/vectors/proto/` decode identically on 32- and 64-bit builds and re-encode to the same bytes; `messages_round_trip_through_cbor` and `junk_and_oversized_input_is_refused` pin the codec; the decoder has a fuzz target.

## Channel

Every TCP connection starts with one plaintext frame, `pr3/0 pair` or `pr3/0 session`. A frame is a big-endian `u32` length and at most 65535 bytes. After the handshake, a message is CBOR split into chunks of at most 65518 bytes; each chunk's plaintext is a flag byte (`1` more, `0` last) and the data, sealed by the Noise transport (16-byte tag) as one frame; the reassembled message is at most 1 MiB. A decryption failure, a bad flag or an oversized message ends the channel; Noise nonces give ordering and replay protection. One message's chunks are never interleaved with another's.

## Pairing

1. The node shows a six-digit code (`u32` from the host's entropy, little-endian, modulo 1 000 000), valid for 120 s. A new code replaces the old one, and the code is consumed **before** the phone proves anything, so each code allows exactly one attempt.
2. The phone strips non-digits and requires six.
3. **SPAKE2** over Ed25519, symmetric variant, identity `pr3-pairing/0`: each side sends `0x53` followed by a 32-byte compressed point; the password scalar is HKDF-SHA256 (empty salt, info `SPAKE2 pw`, 48 bytes) byte-reversed and reduced modulo the group order; the symmetric point `S` is the fixed Ed25519 point of the common SPAKE2 Ed25519 profile, stored in `src/crypto/spake2.c`; the key is SHA-256 over SHA-256(password), SHA-256(identity), the two message bodies sorted, and the shared point.
4. **Noise** `Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s` with the SPAKE2 key as the PSK and the prologue being the pattern name followed by the phone's and the node's SPAKE2 messages. Messages 1 and 2 have empty payloads; message 3 carries the phone's cleaned device name. A wrong code makes message 3 unreadable, and the phone reports `the code was wrong or has expired`.
5. The node stores the phone (name, public key, time) and sends `("paired", <node name>)`; the phone checks the first element and stores the node with its address.

**Sessions** use `Noise_IK_25519_ChaChaPoly_BLAKE2s`: the phone knows the node's static key; the node checks the phone's static key against its list and sends no reply to an unknown key, which the phone reads as "pair again". After the handshake the phone sends `hello`, refuses a different version, and runs one request per session with `id = 1`.

**Implementation.** The Noise state machine (CipherState, SymmetricState, HandshakeState for the two patterns) is PR3's own, about 400 lines, checked against the published Noise test vectors for these suites and between two PR3 builds over loopback. Its primitives come from reviewed code: X25519 and ChaCha20-Poly1305 from mbedTLS, BLAKE2s from the BLAKE2 authors' own C code, and the Edwards-curve arithmetic SPAKE2 needs from libsodium, linked only into profiles that pair ([D-07](19-decisions-and-risks.md)). The device's static X25519 key lives in the secret arena and on disk as `device.key` (hex, created exclusively, mode 0600). Peer lists are written atomically (`*.json.partial`, then rename) as canonical JSON.

## Node and client

| | Value |
| --- | --- |
| Default port and listen address | 7313, `127.0.0.1:7313` |
| Connections handled at once | 8; more are closed |
| Client timeouts | Connect 2 s, handshake 5 s, answer 120 s |
| Node timeouts | Handshake 5 s, idle 120 s, write 120 s, no read deadline while generating |
| Socket options | `TCP_NODELAY` on both sides |
| Cancel | The phone polls its cancel flag every 50 ms and sends `cancel`; the node stops at the next token and answers `failed` with `cancelled` |
| Revocation | Forgetting a phone rewrites the list; the next handshake is refused; a `generate` on an already open connection re-checks the list and fails with `Device revoked` |
| Fallback | Any error but a cancel marks the node unreachable and the phone answers with its own model; labels `"<model> on <node>, else <local>"`, `"<node>, else <local>"`, `"<local> here (<node> unreachable)"` |

The C node runs inside `pr3 node` (CLI) and inside the desktop app. It uses one I/O thread with `poll`/`kqueue`/`epoll`/IOCP readiness over all connections instead of a thread per connection, handing `generate` requests to the inference thread one at a time; a connection's state is a fixed record in a pool of eight.

## Accessory protocol (serial)

For boards that have only a serial port ([part 11](11-embedded-profiles.md)). The accessory talks to a **gateway** on a computer or a Pi it is wired to; the gateway holds the pairing with the owner's node, the credentials and every piece of personal data, and the accessory holds none.

| Layer | Design |
| --- | --- |
| Framing | COBS with `0x00` delimiters, CRC-32C over the frame, at most 256 bytes of payload by default (the board's cap, separate from the protocol's 1 MiB ceiling) |
| Header | Version, type, 16-bit message id, flags |
| Messages | `hello` (boot nonce, board capability manifest), `status`, `ask` (a question, at most 192 bytes), `chunk` (answer text), `done`, `failed`, `cancel`, `credit`, `disconnected` |
| Flow control | Credits granted by the receiver; a full queue withholds credit, never drops |
| Reset | A new boot nonce aborts everything in flight on both sides; the next `ask` starts clean |
| Authority | Read-only requests only; no action, no memory write, no persistence of personal data on the accessory |

The gateway (`pr3 gateway`) bridges `ask` to a `generate` on the owner's node, streams `token` text as `chunk`s within the board's cap, propagates cancellation, and reports an oversized final message or an unreachable node as `failed`, never as a truncated answer that looks complete ([EMB-03](17-delivery-plan.md)).

---

[Back to the index](../README.md)
