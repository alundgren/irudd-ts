import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import * as HttpApi from "effect/unstable/httpapi/HttpApi";
import * as HttpApiGroup from "effect/unstable/httpapi/HttpApiGroup";
import * as HttpApiEndpoint from "effect/unstable/httpapi/HttpApiEndpoint";
import * as HttpApiClient from "effect/unstable/httpapi/HttpApiClient";
import { HttpClient } from "effect/unstable/http";
class EnvironmentAuthHttpApi extends HttpApiGroup.make("auth").add(
  HttpApiEndpoint.post("webSocketTicket", "/api/auth/websocket-ticket", {
    headers: Schema.Struct({ authorization: Schema.optional(Schema.String), dpop: Schema.optional(Schema.String) }),
    success: Schema.Struct({ ticket: Schema.String }),
  }),
) {}
class EnvironmentHttpApi extends HttpApi.make("environment").add(EnvironmentAuthHttpApi) {}
const remoteApiBaseUrl = (url: string) => url;
export const makeEnvironmentHttpApiGroupClient = <
  Group extends keyof typeof EnvironmentHttpApi.groups,
>(
  httpBaseUrl: string,
  group: Group,
) =>
  Effect.flatMap(HttpClient.HttpClient, (httpClient) =>
    HttpApiClient.group(EnvironmentHttpApi, {
      httpClient,
      group,
      baseUrl: remoteApiBaseUrl(httpBaseUrl),
    }),
  );

type PreparedConnection = {httpBaseUrl:string};
namespace Option {export type Option<A> = A | undefined;}
type ManagedRelayDpopSigner = {Service: unknown};
type RemoteEnvironmentAuthorization = {Service: unknown};
namespace HttpMethod {export type HttpMethod = "POST";}
interface EnvironmentHttpAuthHeaders {readonly authorization?:string; readonly dpop?:string;}
declare function executeAuthenticatedEnvironmentHttpRequest<
  Group extends Parameters<typeof makeEnvironmentHttpApiGroupClient>[1],
  A,
  E,
  R,
>(input: {
  readonly prepared: PreparedConnection;
  readonly signer: Option.Option<ManagedRelayDpopSigner["Service"]>;
  readonly remoteAuthorization?: Option.Option<RemoteEnvironmentAuthorization["Service"]>;
  readonly method: HttpMethod.HttpMethod;
  readonly url: (httpBaseUrl: string) => string;
  readonly timeoutMs: number;
  readonly group: Group;
  readonly request: (input: {
    readonly client: Effect.Success<ReturnType<typeof makeEnvironmentHttpApiGroupClient<Group>>>;
    readonly headers: EnvironmentHttpAuthHeaders;
  }) => Effect.Effect<A, E, R>;
  /** Some endpoints report rejected credentials in a successful response. */
  readonly isUnauthorizedResponse?: (response: NoInfer<A>) => boolean;
}): Effect.Effect<A,E,R>;
declare const input: {prepared: PreparedConnection};
declare const signer: undefined;
declare const remoteAuthorization: undefined;
const TICKET_TIMEOUT_MS = 8000;
const environmentEndpointUrl = (base: string, path: string) => base + path;
const ticket = executeAuthenticatedEnvironmentHttpRequest({
      prepared: input.prepared,
      signer,
      remoteAuthorization,
      group: "auth",
      method: "POST",
      url: (httpBaseUrl) => environmentEndpointUrl(httpBaseUrl, "/api/auth/websocket-ticket"),
      timeoutMs: TICKET_TIMEOUT_MS,
      request: ({ client, headers }) => client.webSocketTicket({ headers }),
    });
