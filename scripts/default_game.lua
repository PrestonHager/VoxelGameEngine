-- Example Lua for `VGE_LUA_SCRIPT`. Loaded when the engine starts (embedded or engine-runner).
-- Use level object `instance_id` values as keys in `_instance_hooks`.

-- Optional: called every fixed tick after instance hooks.
function on_tick(dt)
  -- vge-style API is passed only to instance hooks (second argument table).
end

_instance_hooks = {
  -- [1] = function(dt, api)
  --   api.log("hello from instance 1")
  --   local p = api.get_position(1)
  --   if p then
  --     api.set_position(1, p.x, p.y + 0.01 * dt, p.z)
  --   end
  -- end,
}
