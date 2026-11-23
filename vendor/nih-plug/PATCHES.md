# Local nih-plug changes

This is a trimmed vendored copy of
[`robbert-vdh/nih-plug`](https://github.com/robbert-vdh/nih-plug) at
`f36931f7af4646065488a9845d8f8c2f95252c23`.

## VST3 parameter changes when the host stops processing

Some hosts keep a VST3 plugin in its active/processing state while they stop
calling `IAudioProcessor::process()` for a silent plugin. FL Studio does this
with **Allow smart disable** enabled, and REAPER does something similar when a
plugin is bypassed. The VST3 wrapper used to defer GUI parameter changes until
the next process call in that situation, so the editor appeared frozen: clicks
were received and reported to the host, but the host never echoed the change
back through the process callback.

The wrapper now records when `process()` was last called and, if the host is
still marked as processing but no process call has happened for
`PROCESS_CALL_STALL_TIMEOUT_MS` (500 ms), applies GUI/host parameter changes to
the plugin's parameters directly. The normal host echo through the process
callback remains the primary path.

Changed files:

- `src/wrapper/util.rs`: adds `monotonic_millis()`.
- `src/wrapper/vst3/inner.rs`: adds `last_process_at_ms` and
  `should_update_parameters_locally()`.
- `src/wrapper/vst3/context.rs`: uses the helper in
  `raw_set_parameter_normalized()`.
- `src/wrapper/vst3/wrapper.rs`: updates the timestamp from `process()` and
  uses the helper in `set_param_normalized()`.

## VST3/CLAP activation while the audio thread is processing

Some hosts call `IComponent::setActive()` (VST3) or `clap_plugin::activate()`
(CLAP) from a non-audio thread while the audio thread is still inside a
`process()` call for the same plugin instance. Ardour does this on the GUI
thread when a plugin reports a latency change (`restartComponent` with
`kLatencyChanged`), which happens whenever Nebula's lookahead amount changes.

The buffer manager is mutably borrowed for the duration of a process call, so
the unconditional `borrow_mut()` in the activate path hit `atomic_refcell`'s
"already mutably borrowed" panic. Because the panic crosses the host's C ABI
callback it could not unwind, and the host aborted.

The activate paths now use `try_borrow_mut()`. If the audio thread still holds
the borrow, the rebuild is deferred: a `pending_buffer_rebuild` flag is set and
the next `process()` call rebuilds the buffer manager on the audio thread
before processing audio. The activation itself still succeeds.

Changed files:

- `src/wrapper/vst3/inner.rs`: adds `pending_buffer_rebuild`.
- `src/wrapper/vst3/wrapper.rs`: defers the rebuild in `set_active()` and
  performs it at the start of `process()`.
- `src/wrapper/clap/wrapper.rs`: same change for `clap_plugin::activate()` and
  `clap_plugin::process()`.

The copy is otherwise kept in sync with the upstream revision above. The
workspace manifest was trimmed to the crates Nebula De-Esser uses.
