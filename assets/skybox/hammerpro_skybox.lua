HammerProSkybox = HammerProSkybox or {}
local M = HammerProSkybox

M.CopiedKeys = {
	scale = true, angles = true, use_angles = true, fogenable = true, fogblend = true,
	fogcolor = true, fogcolor2 = true, fogdir = true, fogstart = true, fogend = true,
	fogmaxdensity = true, fogradial = true,
}

local function to_number(v, fallback)
	return tonumber(v) or fallback
end

local function parse_vector(text)
	if isvector(text) then return text end
	if not isstring(text) then return nil end
	local x, y, z = string.match(text, "^%s*(%-?[%d%.]+)%s+(%-?[%d%.]+)%s+(%-?[%d%.]+)")
	x, y, z = tonumber(x), tonumber(y), tonumber(z)
	if not (x and y and z) then return nil end
	if math.abs(x) > 65536 or math.abs(y) > 65536 or math.abs(z) > 65536 then return nil end
	return Vector(x, y, z)
end

function M.DataOf(switch)
	if not IsValid(switch) then return nil end
	local keys = switch.HpSky or {}
	local origin = parse_vector(keys.origin)
	local scale = keys.scale
	if not origin and SERVER and isstring(switch.HpSkyName) and switch.HpSkyName ~= "" then
		for _, cam in ipairs(ents.FindByName(switch.HpSkyName)) do
			if IsValid(cam) and cam:GetClass() == "sky_camera" then
				origin = cam:GetPos()
				scale = cam:GetKeyValues() and cam:GetKeyValues().scale
				break
			end
		end
	end
	if not origin then return nil end
	scale = to_number(scale, 16)
	if scale < 1 then scale = 16 end
	return { origin = origin, scale = scale }
end

if SERVER then
	util.AddNetworkString("hp_skybox")

	M.copy = M.copy or nil
	M.active = M.active or nil

	function M.ShowGlobal(switch)
		if IsValid(M.copy) then M.copy:Remove() end
		M.copy = nil
		local previous = M.active
		M.active = nil
		if IsValid(previous) and previous ~= switch then
			previous:TriggerOutput("OnDeactivated", previous)
		end
		if not IsValid(switch) then return true end
		local data = M.DataOf(switch)
		if not data then
			ErrorNoHalt("point_hammerpro_skybox: no sky_camera named '" .. tostring(switch.HpSkyName) .. "'\n")
			return false
		end
		local keys = switch.HpSky or {}
		local cam = ents.Create("sky_camera")
		if not IsValid(cam) then return false end
		for k, v in pairs(keys) do
			if M.CopiedKeys[k] then cam:SetKeyValue(k, v) end
		end
		cam:SetPos(data.origin)
		cam:Spawn()
		cam:Activate()
		M.copy = cam
		M.active = switch
		switch:TriggerOutput("OnActivated", switch)
		return true
	end

	local function send(ply, data)
		if not IsValid(ply) or not ply:IsPlayer() then return end
		net.Start("hp_skybox")
		if data then
			net.WriteBool(true)
			net.WriteVector(data.origin)
			net.WriteFloat(data.scale)
		else
			net.WriteBool(false)
		end
		net.Send(ply)
	end

	function M.ShowForPlayer(ply, switch)
		if not IsValid(ply) or not ply:IsPlayer() then return false end
		local data = IsValid(switch) and M.DataOf(switch) or nil
		if IsValid(switch) and not data then
			ErrorNoHalt("point_hammerpro_skybox: no sky_camera named '" .. tostring(switch.HpSkyName) .. "'\n")
			return false
		end
		local previous = ply.HpSkybox
		ply.HpSkybox = (data and switch) or nil
		send(ply, data)
		if IsValid(previous) and previous ~= switch then
			previous:TriggerOutput("OnDeactivated", ply)
		end
		if IsValid(switch) and data then
			switch:TriggerOutput("OnActivated", ply)
		end
		return true
	end

	hook.Add("PlayerInitialSpawn", "hammerpro_skybox", function(ply)
		ply.HpSkybox = nil
	end)

	return
end

local active = nil
local lastFov = 90

net.Receive("hp_skybox", function()
	if not net.ReadBool() then
		active = nil
		return
	end
	local origin = net.ReadVector()
	local scale = net.ReadFloat()
	if not isvector(origin) or not isnumber(scale) or scale < 1 then
		active = nil
		return
	end
	active = { origin = origin, scale = scale }
end)

hook.Add("CalcView", "hammerpro_skybox_fov", function(_, _, _, fov)
	if fov and fov > 1 then lastFov = fov end
end)

local rendering = false
local rt, rtMat, rtW, rtH

local function target()
	if rt and rtW == ScrW() and rtH == ScrH() then return end
	rtW, rtH = ScrW(), ScrH()
	rt = GetRenderTarget("hammerpro_skybox_" .. rtW .. "x" .. rtH, rtW, rtH)
	rtMat = CreateMaterial("hammerpro_skybox_mat", "UnlitGeneric", {
		["$basetexture"] = "color/white",
		["$ignorez"] = "1",
		["$nolod"] = "1",
		["$vertexcolor"] = "0",
	})
	rtMat:SetTexture("$basetexture", rt)
end

local ready = false

hook.Add("PreRender", "hammerpro_skybox", function()
	ready = false
	if not active or rendering then return end
	rendering = true
	target()
	local scale = 1 / active.scale
	render.PushRenderTarget(rt)
	render.Clear(0, 0, 0, 255, true, true)
	render.RenderView({
		origin = active.origin + EyePos() * scale,
		angles = EyeAngles(),
		aspectratio = ScrW() / ScrH(),
		x = 0,
		y = 0,
		w = rtW,
		h = rtH,
		fov = lastFov,
		drawhud = false,
		drawviewmodel = false,
		drawmonitors = false,
		dopostprocess = false,
		bloomtone = false,
		znear = 3,
		zfar = 32768,
	})
	render.PopRenderTarget()
	rendering = false
	ready = true
end)

hook.Add("PreDrawSkyBox", "hammerpro_skybox", function()
	if not active or not ready then return end
	render.SetMaterial(rtMat)
	render.DrawScreenQuad()
	return true
end)
