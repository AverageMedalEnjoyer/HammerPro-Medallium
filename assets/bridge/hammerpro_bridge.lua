if not SERVER or game.IsDedicated() then return end

local script_start = SysTime()

local DIR = "hammerpro"
local CMD = DIR .. "/cmd.json"
local ACK = DIR .. "/ack.json"
local STATE = DIR .. "/state.json"
local PROPS = DIR .. "/props.json"

file.CreateDir(DIR)

local function read_json(path)
	local text = file.Read(path, "DATA")
	if not text or #text == 0 or #text > 65536 then return nil end
	local ok, value = pcall(util.JSONToTable, text)
	if ok and istable(value) then return value end
	return nil
end

local function write_json(path, value)
	file.Write(path, util.TableToJSON(value) or "{}")
end

local state = read_json(STATE) or {}
local last_seq = tonumber(state.last_seq) or 0

local function save_state()
	write_json(STATE, state)
end

local function safe_map(name)
	return isstring(name) and #name > 0 and #name <= 64 and name:match("^[%w_%-]+$") ~= nil
end

local function numbers(t, limit)
	if not istable(t) then return nil end
	local a, b, c = tonumber(t[1]), tonumber(t[2]), tonumber(t[3])
	if not (a and b and c) then return nil end
	if math.abs(a) > limit or math.abs(b) > limit or math.abs(c) > limit then return nil end
	return a, b, c
end

local function ack(placed, spawned)
	write_json(ACK, {
		seq = tostring(last_seq),
		map = game.GetMap(),
		placed = placed and true or false,
		spawned = spawned and true or false,
		lua_ms = spawned and math.floor((SysTime() - script_start) * 1000) or nil,
		time = os.time(),
	})
end

local function place(ply, pos, ang)
	local x, y, z = numbers(pos, 65536)
	if x then
		ply:SetMoveType(MOVETYPE_NOCLIP)

		ply:SetPos(Vector(x, y, z - 64))
	end
	local p, yw, r = numbers(ang, 360000)
	if p then ply:SetEyeAngles(Angle(p, yw, r)) end
end

local function dump_props()
	local out = {}
	for _, ent in ipairs(ents.GetAll()) do
		local class = ent:GetClass()
		local wanted = class == "prop_physics" or class == "prop_dynamic" or class == "prop_physics_multiplayer"
		if wanted and not ent:CreatedByMap() then
			local pos, ang = ent:GetPos(), ent:GetAngles()
			out[#out + 1] = {
				class = class,
				model = ent:GetModel() or "",
				skin = ent:GetSkin() or 0,
				origin = { pos.x, pos.y, pos.z },
				angles = { ang.p, ang.y, ang.r },
			}
			if #out >= 4096 then break end
		end
	end
	write_json(PROPS, { map = game.GetMap(), props = out })
end

local function poll()
	local cmd = read_json(CMD)
	local seq = cmd and tonumber(cmd.seq)
	if not seq or seq <= last_seq then return end
	last_seq = seq

	state.last_seq = tostring(seq)
	if safe_map(cmd.map) and (cmd.setpos or cmd.setang) then
		state.pending = { map = cmd.map, setpos = cmd.setpos, setang = cmd.setang }
	end
	if cmd.dump_props == true then dump_props() end
	save_state()
	ack(false, false)
end

poll()
timer.Create("hammerpro_bridge_poll", 0.5, 0, poll)

hook.Add("PlayerInitialSpawn", "hammerpro_bridge_spawn", function(ply)

	timer.Simple(0, function()
		if not IsValid(ply) or not (ply:IsListenServerHost() or game.SinglePlayer()) then return end
		local pending = state.pending
		local placed = false
		if istable(pending) and pending.map == game.GetMap() then
			place(ply, pending.setpos, pending.setang)
			state.pending = nil
			save_state()
			placed = true
		end
		ack(placed, true)
	end)
end)
