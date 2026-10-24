import { Schema } from "effect";

export const CAP_PUSH = "push.v1";

export const PushHandle = Schema.Struct({ handle: Schema.NonEmptyString, secret: Schema.NonEmptyString, expiresAt: Schema.Int });
export const PushPrefs = Schema.Struct({
  needsYou: Schema.Boolean,
  done: Schema.Boolean,
  failed: Schema.Boolean,
  automations: Schema.Boolean,
  details: Schema.Boolean,
});

export const PushClient = [
  Schema.Struct({
    type: Schema.Literal("push.register"),
    id: Schema.String,
    key: Schema.String,
    env: Schema.Literal("development", "production"),
    alert: PushHandle,
    start: Schema.optional(PushHandle),
    prefs: PushPrefs,
    frequent: Schema.optional(Schema.Boolean),
  }),
  Schema.Struct({
    type: Schema.Literal("push.activity"),
    id: Schema.String,
    activityId: Schema.NonEmptyString,
    handle: Schema.optional(PushHandle),
    ended: Schema.optional(Schema.Boolean),
  }),
  Schema.Struct({ type: Schema.Literal("push.prefs"), id: Schema.String, prefs: PushPrefs }),
  Schema.Struct({ type: Schema.Literal("push.unregister"), id: Schema.String }),
  Schema.Struct({ type: Schema.Literal("push.test"), id: Schema.String }),
] as const;
