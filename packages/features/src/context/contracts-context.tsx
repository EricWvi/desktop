import { createContext, useContext, useMemo, type ReactNode } from "react";
import { createContractsClient, type ContractsClient } from "@ora/contracts";
import { createFetchTransport } from "@ora/contracts/fetch";

const ContractsClientContext = createContext<ContractsClient | null>(null);

export function ContractsClientProvider({ children }: { children: ReactNode }) {
  const client = useMemo(() => createContractsClient(createFetchTransport()), []);
  return (
    <ContractsClientContext.Provider value={client}>
      {children}
    </ContractsClientContext.Provider>
  );
}

export function useContractsClient(): ContractsClient {
  const client = useContext(ContractsClientContext);
  if (!client) throw new Error("useContractsClient must be used within ContractsClientProvider");
  return client;
}
