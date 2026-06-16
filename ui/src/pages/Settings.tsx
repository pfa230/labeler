import { SettingsSection } from "./settings/SettingsSection";
import { PrintersSection } from "./settings/PrintersSection";

export function Settings() {
  return (
    <div className="flex flex-col gap-8">
      <h1 className="text-2xl font-semibold">Settings</h1>
      <SettingsSection />
      <PrintersSection />
    </div>
  );
}
