import { createContractsClient } from "@ora/contracts";
import { createFetchTransport } from "@ora/contracts/fetch";
import { AppShell } from "@ora/features";

const client = createContractsClient(createFetchTransport());

export default function App() {
  return <AppShell client={client} />;
}
