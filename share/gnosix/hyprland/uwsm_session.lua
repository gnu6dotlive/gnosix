-- GNOSIX's narrow Hyprland-to-UWSM session integration.
-- The Hyprland root remains user-owned; UWSM owns session finalization.
local M = {}

M.minimum_uwsm_version = "0.27.0"
M.finalize_command = "/usr/bin/uwsm finalize XDG_CURRENT_DESKTOP QT_QPA_PLATFORM XDG_SESSION_TYPE QT_QPA_PLATFORMTHEME"

function M.register(hl)
    assert(type(hl) == "table", "Hyprland Lua API table required")
    assert(type(hl.on) == "function", "Hyprland event registration unavailable")
    assert(type(hl.exec_cmd) == "function", "Hyprland command callback unavailable")

    hl.on("hyprland.start", function()
        hl.exec_cmd(M.finalize_command)
    end)
end

return M
