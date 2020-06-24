# TODO
- Add log crate
- Syncronize the frames from the server with the client
- #derive(Delta)
- When getting a delta, it should return a Result<Delta>
- Add custom serialization to Bitvec(sparce set), for better space efficience

## Client
- Add command list in client state, make sure the commands are acked and valid in server, always send unacked commands, invalidate really old commands

## Server

## Bugs
- After 200 frames or so the client is disconnected and reconnected instantly

## Ideas
  - Implement trait that has trait type, that chooses how to serialize the world and returns type A, and choose what to send to player X from type A
    - Default implementation would be serialize whole world and send to every player the same thing
    - ```rust
      trait NetworkSerializable {
        type A;
        type B;
        
        fn serialize(world: &World) -> A;
        fn select_for_client(client: NetworkClient, serialization: A) -> B;
      }
      ```
