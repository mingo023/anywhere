import { Data } from "effect";

export class ConfigInvalid extends Data.TaggedError("ConfigInvalid")<{ readonly reason: string }> {}
export class UnknownProfile extends Data.TaggedError("UnknownProfile")<{ readonly profileId: string }> {}
export class UnknownProvider extends Data.TaggedError("UnknownProvider")<{ readonly provider: string }> {}
export class UnknownAgent extends Data.TaggedError("UnknownAgent")<{ readonly agentId: string }> {}
export class ProviderFailure extends Data.TaggedError("ProviderFailure")<{ readonly message: string }> {}

export type AppError =
  | ConfigInvalid
  | UnknownProfile
  | UnknownProvider
  | UnknownAgent
  | ProviderFailure;

export function describe(error: AppError): string {
  switch (error._tag) {
    case "ConfigInvalid":
      return `Invalid config: ${error.reason}`;
    case "UnknownProfile":
      return `Unknown profile: ${error.profileId}`;
    case "UnknownProvider":
      return `Unknown provider: ${error.provider}`;
    case "UnknownAgent":
      return `Unknown agent: ${error.agentId}`;
    case "ProviderFailure":
      return error.message;
  }
}
