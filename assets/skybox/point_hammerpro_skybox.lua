ENT.Type = "point"
ENT.DisableDuplicator = true

if CLIENT then return end

function ENT:KeyValue(key, value)
	local k = string.lower(key)
	if k == "skycamera" then
		self.HpSkyName = value
	elseif k == "mode" then
		self.HpPerPlayer = tonumber(value) == 1
	elseif string.sub(k, 1, 4) == "sky_" then
		self.HpSky = self.HpSky or {}
		self.HpSky[string.sub(k, 5)] = value
	elseif k == "spawnflags" then
		self.HpFlags = tonumber(value) or 0
	elseif string.sub(k, 1, 2) == "on" then
		self:StoreOutput(key, value)
	end
end

function ENT:Apply(show, ply)
	local M = HammerProSkybox
	if not M then return end
	if self.HpPerPlayer then
		M.ShowForPlayer(IsValid(ply) and ply or nil, show and self or nil)
	else
		M.ShowGlobal(show and self or nil)
	end
end

function ENT:IsShown(ply)
	local M = HammerProSkybox
	if not M then return false end
	if self.HpPerPlayer then
		return IsValid(ply) and ply.HpSkybox == self
	end
	return M.active == self
end

function ENT:Initialize()
	if bit.band(self.HpFlags or 0, 1) ~= 0 and not self.HpPerPlayer then

		timer.Simple(0, function()
			if IsValid(self) then self:Apply(true) end
		end)
	end
end

function ENT:AcceptInput(name, activator, caller, data)
	local n = string.lower(name)
	if n == "activate" then
		self:Apply(true, activator)
		return true
	elseif n == "deactivate" then
		if self:IsShown(activator) then self:Apply(false, activator) end
		return true
	elseif n == "toggle" then
		self:Apply(not self:IsShown(activator), activator)
		return true
	end
	return false
end

function ENT:OnRemove()
	local M = HammerProSkybox
	if M and not self.HpPerPlayer and M.active == self then
		M.ShowGlobal(nil)
	end
end
