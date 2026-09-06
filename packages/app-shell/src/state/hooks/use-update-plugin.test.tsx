import { act, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  createTestClient,
  type TestHandlers,
} from "../../test/contracts-transport";
import { createPluginMemory, pluginHandlers } from "../../test/memory/plugins";
import "../../i18n/i18n-instance";
import { renderHookWithClient } from "../../test/hook-harness";
import { useUpdatePlugin } from "./use-update-plugin";

/** State for this test surface; no unrelated domain fixtures are initialized. */
function createFixtureState() {
  return { ...createPluginMemory() };
}

type FixtureState = ReturnType<typeof createFixtureState>;

/** Explicit domain composition for the behaviors exercised by this test file. */
function createFixtureHandlers(state: FixtureState): TestHandlers {
  return {
    ...pluginHandlers(state),
  };
}

describe("useUpdatePlugin", () => {
  it("updates an installed plugin and refreshes the installed surface", async () => {
    const state = createFixtureState();
    state.availablePlugins.push({
      id: "official/weather",
      name: "weather",
      title: "Weather",
      kind: "agent",
      namespace: "official",
      sourceUrl: "https://github.com/ora-space/marketplace",
      version: "1.1.0",
      description: "Weather",
      logo: null,
      compatibility: "compatible",
    });
    state.installedPlugins.push({
      id: "official/weather",
      namespace: "official",
      name: "weather",
      displayName: "weather",
      version: "1.0.0",
      description: "Weather",
      homepage: null,
      license: null,
      kind: "agent",
      agentDisplayName: "weather",
      logo: null,
      installationValidity: { validity: "valid" },
      configuration: { state: "not_declared" },
      runtime: "stopped",
    });
    const clientHandlers: TestHandlers = createFixtureHandlers(state);
    const client = createTestClient(clientHandlers);
    const { result } = renderHookWithClient(
      () => useUpdatePlugin("official/weather"),
      client,
    );

    act(() => result.current.mutate({}));

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(
      state.installedPlugins.find((item) => item.id === "official/weather"),
    ).toMatchObject({
      id: "official/weather",
      version: "1.1.0",
    });
  });
});
