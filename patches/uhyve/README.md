# uhyve — no patch needed (use stock ≥ 0.9.1)

**There is no uhyve fork or patch here, and none is required.**

The relay maps a real TLS cert + auth key into the guest with a **directory** mapping
(`--file-mapping <hostdir>:/certs`). This is a **first-class upstream feature of stock uhyve
≥ 0.9.1**, documented in `uhyve --help`:

```
--file-mapping <FILE_MAPPING>
    Example: --file-mapping host_dir:guest_dir --file-mapping file.txt:guest_file.txt
```

The earlier symptom — "single-file `--file-mapping` to the guest root ENOENTs" — was a limitation of
the **0.8.0** uhyve the fleet originally ran, **fixed by upgrading to 0.9.1**, not by patching. The
binary once labeled `uhyve-patched` is verified stock uhyve `0.9.1` from crates.io (all source paths
resolve to `.../registry/.../uhyve-0.9.1/...`; no local modifications).

**Requirement for this project:** stock **uhyve ≥ 0.9.1** + a HermitOS kernel built with the `fs`
feature (see `../hermit-kernel/` and `UPSTREAM.md`). Nothing to upstream.
