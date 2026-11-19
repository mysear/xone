# Ethereum mainnet historical sync committee handovers

The test reads archived Beacon REST data from `ethereum-beacon-api.publicnode.com`. For each case the bootstrap is the standard `light_client/bootstrap/{root}` response for the first update's finalized beacon block. The test constants fix the checkpoint and expected execution block. The last update supplies the signed finality fields. These captures exercise historical verification.

| Fixture | Update periods used in order | Pinned bootstrap beacon root |
|---|---|---|
| `deneb-bootstrap.json.gz` | 1421 (bootstrap rejected at the committee branch) | `0xcda3a7b63f1a5c787e9ad8ae955e8fceff45b54e58d890cd6eb0d80486782598` |
| `electra-fulu.json.gz` | 1606, 1607 | `0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c` |

`fulu-rotation-bootstrap.json.gz` contains the bootstrap for finalized slot 13171968, root `0xaa34231b2a668decf756fab0a13b61a96d7d3108afebfc269f2b350e49dc5a38`. The rotation tests join it to the signed finality in the Electra to Fulu capture.
