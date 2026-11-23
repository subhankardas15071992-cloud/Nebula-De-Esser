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

The copy is otherwise kept in sync with the upstream revision above. The
workspace manifest was trimmed to the crates Nebula De-Esser uses.
