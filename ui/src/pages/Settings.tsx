import { useAuth } from "../api/auth";
import { VariablesSection } from "./settings/VariablesSection";
import { PrintersSection } from "./settings/PrintersSection";
import { ConnectionsSection } from "./settings/ConnectionsSection";
import { UsersSection } from "./settings/UsersSection";
import { TokensSection } from "./settings/TokensSection";
import { SettingsSection } from "./settings/SettingsSection";

export function Settings() {
  const { data: auth } = useAuth();
  return (
    <div className="flex max-w-3xl flex-col gap-8">
      <h1 className="text-2xl font-semibold">Settings</h1>
      <VariablesSection />
      <SettingsSection />
      <PrintersSection />
      <ConnectionsSection />
      {!auth?.noAuth && (
        <>
          <UsersSection />
          <TokensSection />
        </>
      )}
    </div>
  );
}
