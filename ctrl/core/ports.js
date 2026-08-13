export { WorkDiscovery } from "../sdk/index.js";

export async function invokePort(resolution, port, operation, input) {
  const contract = port.operations[operation];
  if (!contract) throw new TypeError(`Port ${port.id} does not define operation ${operation}.`);
  contract.validateInput(input);
  const implementation = resolution.connector.implementations[port.id]?.[operation];
  if (typeof implementation !== "function") {
    throw new TypeError(`Connector ${resolution.connector.manifest.id} does not implement ${port.id}.${operation}.`);
  }
  const output = await implementation(input);
  contract.validateOutput(output);
  return output;
}
