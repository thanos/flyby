# Expected output (approximate)

```text
Running scenario 'constant_rate': Steady 100 kpps, no faults, 1 second virtual time.
  ...
  Note     : results are SIMULATED (not hardware)

Results (simulated):
  Ticks             : 1000
  Packets generated : 100000
  Packets dropped   : 0
  ...
  Throughput        : <machine-dependent> pps (wall-clock, simulated)
```

Throughput is **simulated** and machine-dependent; packet counts for this
scenario should be stable under virtual time.
