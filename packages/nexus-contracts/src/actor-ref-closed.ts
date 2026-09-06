import type { ActorRef } from './generated/domain/actor-ref';

export const creatorOk: ActorRef = {
  actor_kind: 'creator',
  creator_id: 'ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
};

export const characterOk: ActorRef = {
  actor_kind: 'character',
  character_id: 'chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
};

export const unknownKind: ActorRef = {
  // @ts-expect-error unknown actor_kind is not a closed ActorRef discriminant
  actor_kind: 'npc',
  creator_id: 'ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
};

export const wrongArm: ActorRef = {
  actor_kind: 'creator',
  // @ts-expect-error creator discriminant cannot carry a character_id arm
  character_id: 'chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
};
