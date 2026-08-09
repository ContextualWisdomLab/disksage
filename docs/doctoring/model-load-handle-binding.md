# Verified model handle binding through llama.cpp load

## Decision

DiskSage retains the exact open file that passed load-time model verification until `LlamaModel::load_from_file` has finished opening and parsing the artifact. Verification and model parsing are therefore one continuous authority interval rather than two unrelated pathname opens.

The design is platform-specific because `llama-cpp-2` 0.1.151 exposes `LlamaModel::load_from_file`, whose public contract accepts a filesystem `Path`; it does not expose a `File`/file-descriptor loading API. DiskSage does not bypass that wrapper with an unreviewed unsafe FFI call.

### Unix

After non-following pathname admission, DiskSage opens the installed model once, verifies the opened regular file type and trusted byte length, binds the opened handle to the current pathname with `same_file::Handle`, hashes the actual bytes, rewinds the retained file, and passes llama.cpp a **stable descriptor path** while that handle remains alive:

- Linux: `/proc/self/fd/<fd>`;
- other supported Unix targets, including macOS: `/dev/fd/<fd>`.

Replacing, renaming, or redirecting the original pathname after verification therefore does not retarget the descriptor path used by llama.cpp. The retained file itself is the authority.

### Windows

Rust's Windows `OpenOptionsExt::share_mode` documentation states that the default file sharing mode permits other processes to read, write, delete, and rename an open file, and that removing share flags prevents the corresponding operations while the handle remains open. DiskSage therefore opens the verified source with `FILE_SHARE_READ` only. The verified read handle remains alive through `LlamaModel::load_from_file`, allowing llama.cpp to obtain another read handle while preventing concurrent write and delete/rename access to that file during the load interval. This is the **Windows read-sharing guard**.

## Identity and integrity contract

The load guard applies all of the following before returning a load path:

1. inspect the requested path with non-following metadata;
2. reject symbolic links, non-regular files, and trusted-size drift before opening;
3. open the candidate once using the platform-specific guard;
4. re-check the opened handle's regular-file type and exact length;
5. re-check the pathname after open and reject a newly substituted symbolic link or non-regular object;
6. compare a `same_file::Handle` made from the retained open file with a handle made from the current path;
7. recompute exact byte count and SHA-256 from the retained file through the existing 64 KiB buffer;
8. rewind the retained file; and
9. keep the guard alive until llama.cpp's path-based load returns.

A namespace race detected between the opened identity and current pathname fails closed with `model-installed-identity-mismatch`. Existing path-free error codes remain unchanged for missing, non-regular, size, read, and digest failures.

`same-file` documents `Handle` as a cross-platform file-identity abstraction, using device/inode identity on Unix and file identifiers/volume information on Windows. The identity comparison is an admission control, while the retained descriptor or Windows sharing guard protects the subsequent load interval.

## Threat and limitation boundary

This control closes the verified-path-to-loader pathname replacement window identified during current-head review. It does not claim that a process with stronger local privileges, kernel authority, debugger/ptrace capability, or the ability to subvert the operating system cannot alter process memory or defeat local file controls. Those capabilities are outside the ordinary same-user filesystem namespace race addressed by this slice and require OS sandboxing, code signing, endpoint protection, and platform policy rather than a pathname verifier.

DiskSage deliberately does not create an additional 1.12 GB temporary snapshot on every startup merely to obtain a second pathname. That would increase disk-pressure failure risk and SSD writes in a disk-reclamation product. The retained-handle design instead removes namespace retargeting while preserving the existing single verification read plus llama.cpp load.

## Verification

Deterministic tests cover the authority transition rather than timing-sensitive sleeps:

- the existing injected-opener tests retain pre-open fail-closed behavior;
- on Unix, a verified model is prepared, the original source pathname is renamed, an attacker-controlled replacement is created at the old name, and reading the guard's load path still returns the verified bytes;
- on Windows, the retained read-sharing guard must reject write and delete attempts while the load guard is alive;
- engine source regression requires the retained `VerifiedInstalledModel` to be created before backend initialization and requires `LlamaModel::load_from_file` to receive `verified_model.load_path()` rather than the mutable input pathname.

Exact-current-head Test, Release, Security Scan, SAST, coverage, review, packaging/provenance, and repository-policy evidence remain required. A predecessor review or successful run does not authorize the changed head.

## Rollback

Rollback must never restore the old verify-then-reopen pathname gap. If a supported platform cannot expose a stable descriptor path or Windows sharing behavior proves incompatible with llama.cpp, replace this mechanism only with an equally strong or stronger reviewed binding such as a loader API that accepts an already-open handle, or a platform-native immutable snapshot whose resource cost is explicitly accepted. Add a failing race regression before changing the production boundary.

## References

llama-cpp-2 contributors. (2026). *LlamaModel::load_from_file* (llama-cpp-2 Rust crate documentation). Docs.rs. Retrieved August 10, 2026, from https://docs.rs/llama-cpp-2/latest/llama_cpp_2/model/struct.LlamaModel.html

same-file contributors. (n.d.). *same-file 1.0.6: Handle* [Rust crate documentation]. Docs.rs. Retrieved August 10, 2026, from https://docs.rs/same-file/1.0.6/same_file/struct.Handle.html

The Rust Project Developers. (2026). *OpenOptionsExt in std::os::windows::fs*. Rust Standard Library 1.97.1 documentation. Retrieved August 10, 2026, from https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html

## Evidence verification note

The public `llama-cpp-2` load API, `same-file` 1.0.6 handle semantics, and Rust 1.97.1 Windows sharing-mode documentation were rechecked against their primary technical documentation on August 10, 2026. These references describe API behavior; they do not imply a certification or platform security guarantee beyond the tested authority boundary.