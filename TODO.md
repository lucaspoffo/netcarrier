# TODO
- Add log crate

## Client
- Syncronize entities when they are removed in the server
- Add command list in client state, make sure the commands are acked and valid in server, always send unacked commands, invalidate really old commands

## Server
- Use deltas packets and full state packet (before doing make sure to benchmark how much network we will be saving and the cost for calculating deltas)
  - Would have 2 types of packets FullPacket and DeltaPacket
  - All components would implement an Trait that world compare last main frame value tu current and return the delta type with the delta value
  - Default behaviour would be to implement the trait returning the current value (could do a derive macro for faster prototyping)
  - Investigate how would we create a new component/entity in the delta packet. (Create a mini full packet with created components)
- Make bit mask for acked packets
  - Use unacked packets to recend removed entities, maybe unnecessary when we use deltas packets (remove all entities that have NetworkIdentifier and not in the full state packet)

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
