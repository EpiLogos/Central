export type MutationScope = "read-only" | "locally-mutating" | "externally-mutating";

export interface WorkItem {
  name: string;
  path: string;
}

export interface WorkDiscoveryListInput {
  workRoot: string;
}

export interface WorkDiscoveryListOutput {
  items: WorkItem[];
}

export interface CapabilityProbeResult {
  available: boolean;
  reason?: string;
}

export interface PortDeclaration {
  id: string;
  version: string;
}

export interface ConnectorManifest {
  apiVersion: "central.connector/v1";
  id: string;
  version: string;
  displayName: string;
  ports: PortDeclaration[];
  platforms: string[];
  runtimeRequirements: string[];
  dependencyProbes: string[];
  configurationRequirements: string[];
  mutationScope: MutationScope;
}

export interface WorkDiscoveryImplementation {
  list(input: WorkDiscoveryListInput): Promise<WorkDiscoveryListOutput>;
}

export interface Connector {
  manifest: ConnectorManifest;
  probe(context: { port: string; platform: string }): Promise<CapabilityProbeResult>;
  implementations: Record<string, unknown> & {
    WorkDiscovery?: WorkDiscoveryImplementation;
  };
}

export interface PortContract<I, O> {
  id: string;
  version: string;
  purpose: string;
  mutationClass: MutationScope;
  operations: Record<string, {
    inputType: string;
    outputType: string;
    validateInput(input: unknown): asserts input is I;
    validateOutput(output: unknown): asserts output is O;
    deterministic: boolean;
    idempotent: boolean;
  }>;
}

export const CONNECTOR_API_VERSION: "central.connector/v1";
export const WorkDiscovery: PortContract<WorkDiscoveryListInput, WorkDiscoveryListOutput>;
export function defineConnector<T extends Connector>(connector: T): T;
export function validateConnector(connector: unknown): Connector;
export function validateConnectorManifest(manifest: unknown): ConnectorManifest;
export function runWorkDiscoveryConformance(
  connector: Connector,
  fixture: { workRoot: string; expectedNames?: string[] },
): Promise<{
  ok: true;
  port: PortDeclaration;
  connector: { id: string; version: string };
  checks: string[];
}>;
