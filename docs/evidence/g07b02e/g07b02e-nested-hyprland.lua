-- G07B02E disposable nested compositor: output geometry from parent, trace enabled.
hl.monitor({ output = "", mode = "preferred", position = "0x0", scale = 1 })
hl.config({
  animations = { enabled = false },
  misc = { disable_hyprland_logo = true, disable_splash_rendering = true },
  input = { follow_mouse = 1 },
  debug = { disable_logs = false, enable_stdout_logs = true }
})
