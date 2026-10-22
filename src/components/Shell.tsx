import { NavLink, Outlet, useLocation } from "react-router-dom";
import { Download, Gamepad2, HardDrive, Settings, Sparkles } from "lucide-react";
import { t } from "../i18n";

const TABS = [
  { to: "/launch", key: "nav.launch", Icon: Gamepad2 },
  { to: "/download", key: "nav.download", Icon: Download },
  { to: "/instances", key: "nav.instances", Icon: HardDrive },
  { to: "/settings", key: "nav.settings", Icon: Settings },
  { to: "/others", key: "nav.others", Icon: Sparkles },
] as const;

/** Window shell: draggable title bar with top tabs; pages render their own side nav. */
export function Shell() {
  const loc = useLocation();
  return (
    <div className="shell">
      <header className="titlebar" data-tauri-drag-region>
        <span className="brand" data-tauri-drag-region>{t("app.name")}</span>
        <nav className="titlebar-nav" aria-label="主导航">
          {TABS.map(({ to, key, Icon }) => (
            <NavLink key={to} to={to} className={({ isActive }) => `tab ${isActive ? "active" : ""}`}>
              <Icon size={16} aria-hidden /> {t(key)}
            </NavLink>
          ))}
        </nav>
        <span style={{ width: 80 }} />
      </header>
      <div className="body">
        <main className="content page-enter" key={loc.pathname.split("/")[1]} style={{ padding: 0, display: "flex" }}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}

/** Two-pane layout used by pages with a left side nav. */
export function SplitPage({ side, children }: { side: React.ReactNode; children: React.ReactNode }) {
  return (
    <>
      <aside className="sidenav">{side}</aside>
      <div className="content grow">{children}</div>
    </>
  );
}

export function SideLink({ to, children, end }: { to: string; children: React.ReactNode; end?: boolean }) {
  return <NavLink to={to} end={end} className={({ isActive }) => (isActive ? "active" : "")}>{children}</NavLink>;
}
