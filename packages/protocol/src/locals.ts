import { Schema } from "effect";

export const LocalId = Schema.String.pipe(Schema.pattern(/^[A-Za-z0-9_-]{1,64}$/));

export const Local = Schema.Struct({ id: Schema.String, project: Schema.String, name: Schema.String });
export type Local = typeof Local.Type;
