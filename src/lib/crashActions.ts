/** Maps the crash module's suggested action ids to something the UI can do. */
export type CrashActionPlan =
  | { kind: "route"; route: string }
  | { kind: "open"; target: "instance" | "game" | "mods" | "saves" | "logs" }
  | { kind: "none" };

export function planCrashAction(id: string): CrashActionPlan {
  switch (id) {
    case "edit_jvm_args":
    case "increase_memory":
    case "change_java":
    case "download_java":
      return { kind: "route", route: "/settings/java" };
    case "open_mods_folder":
    case "disable_mods":
    case "remove_mods":
    case "install_dependency":
      return { kind: "open", target: "mods" };
    case "remove_resourcepack":
    case "remove_heavy_content":
      return { kind: "open", target: "game" };
    case "reinstall_loader":
    case "change_optifine":
    case "redownload_game":
    case "reinstall_natives":
      return { kind: "route", route: "/download/game" };
    default:
      return { kind: "none" };
  }
}
