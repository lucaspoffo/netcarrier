# TODO
- Add log crate

## Client
- Add jit buffer for the incoming packets in client
- Add command list in client state, make sure the commands are acked and valid in server, always send unacked commands, invalidate really old commands

## Server
- Use deltas packets and full state packet (before doing make sure to benchmark how much network we will be saving and the cost for calculating deltas)
- Make bit mask for acked packets
  - Use unacked packets to recend removed entities, maybe unnecessary when we use deltas packets (remove all entities that have NetworkIdentifier and not in the full state packet)
