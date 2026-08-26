## Contributions

- **Dataset**: 400K clips drawn from public corpora
- **Benchmark**: 16 subtasks organised in 3 levels:
  - **Basic**: detect sources, estimate direction
  - **Relations**: compare left/right and distance
- **Baselines**: several open models

1. Freeze the backbone and train only the projector
2. Enable adapters and train them jointly
3. Unfreeze and fine-tune end to end
