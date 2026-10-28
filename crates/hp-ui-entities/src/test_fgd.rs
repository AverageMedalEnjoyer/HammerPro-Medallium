use hp_fgd::{Fgd, ResolvedFgd};

pub const TEST_FGD: &str = r#"
@BaseClass = Targetname
[
    targetname(target_source) : "Name" : : "The name that other entities refer to this entity by."
    input Kill(void) : "Removes this entity from the world."
    output OnUser1(void) : "Fired in response to FireUser1 input."
]
@BaseClass = Angles
[
    angles(angle) : "Pitch Yaw Roll (Y Z X)" : "0 0 0" : "This entity's orientation in the world."
]
@SolidClass base(Targetname) = worldspawn : "This is the world entity."
[
    message(string) : "Map Description / Title"
    skyname(string) : "SkyBox Texture Name" : "sky_day01_01"
    maxpropscreenwidth(float) : "Start Fade Pixels" : -1
    detailvbsp(string) : "Detail.vbsp file" : "detail.vbsp"
    detailmaterial(string) : "Detail material file" : "detail/detailsprites"
    startdark(choices) : "Level Fade In" : 0 = [ 0 : "No" 1 : "Yes" ]
]
@PointClass base(Angles) studio("models/editor/playerstart.mdl") = info_player_start : "Player spawn point."
[
    spawnflags(flags) = [ 1 : "Master" : 0 ]
]
@PointClass base(Targetname, Angles) iconsprite("editor/light.vmt") = light : "An invisible omnidirectional light source."
[
    _light(color255) : "Brightness" : "255 255 255 200" : "Color and brightness of the light."
    style(choices) : "Appearance" : 0 = [ 0 : "Normal" 10 : "Fluorescent flicker" 2 : "Slow, strong pulse" ]
    _distance(integer) : "Maximum Distance" : 0
    _fifty_percent_distance(float) : "50 percent falloff distance" : "0"
    spawnflags(flags) = [ 1 : "Initially dark" : 0 ]
    input TurnOn(void) : "Turn the light on."
    input TurnOff(void) : "Turn the light off."
    input Toggle(void) : "Toggle the light."
]
@PointClass base(Targetname, Angles) = light_spot : "A cone of light."
[
    _light(color255) : "Brightness" : "255 255 255 200"
    _cone(integer) : "Outer (fading) angle" : 45
    pitch(angle_negative_pitch) : "Pitch" : -90
]
@SolidClass base(Targetname) = trigger_multiple : "A trigger volume that fires repeatedly."
[
    spawnflags(flags) = [ 1 : "Clients" : 1 2 : "NPCs" : 0 4 : "Pushables" : 0 ]
    wait(float) : "Delay Before Reset" : 1 : "Seconds before the trigger can fire again."
    filtername(filterclass) : "Filter Name"
    output OnTrigger(void) : "Fired whenever the trigger is activated."
    output OnStartTouch(void) : "Fired when an entity starts touching the trigger."
    input Enable(void) : "Enable the trigger."
    input Disable(void) : "Disable the trigger."
]
@SolidClass = func_detail : "Detail brushes, ignored by visibility." []
@SolidClass base(Targetname) = func_brush : "A toggleable brush." [ solidity(choices) : "Solidity" : 0 = [ 0 : "Toggle" 1 : "Never solid" 2 : "Always solid" ] ]
@PointClass base(Targetname, Angles) studioprop() = prop_static : "A static prop."
[
    model(studio) : "World Model" : : "The model."
    skin(integer) : "Skin" : 0
    solid(boolean) : "Solid" : 1
]
@PointClass base(Targetname, Angles) studioprop() = prop_physics : "A physics prop."
[
    model(studio) : "World Model"
    skin(integer) : "Skin" : 0
]
@PointClass base(Targetname) = ambient_generic : "Plays a sound."
[
    message(sound) : "Sound Name" : "" : "Name of the sound."
    health(integer) : "Volume" : 10
    SourceEntityName(target_destination) : "Source Entity"
]
@PointClass base(Targetname) = env_sprite : "A sprite."
[
    model(sprite) : "Sprite Name" : "sprites/glow01.spr"
    rendercolor(color255) : "Color (R G B)" : "255 255 255"
]
@NPCClass base(Targetname, Angles) = npc_citizen : "A citizen." []
@FilterClass base(Targetname) = filter_activator_name : "A name filter." [ filtername(target_destination) : "Filter Name" ]
@PointClass base(Targetname) = point_everything : "Every property type."
[
    s(string) : "String"
    i(integer) : "Integer" : 5
    f(float) : "Float" : "0.5"
    b(boolean) : "Boolean" : 0
    c1(color1) : "Color 1" : "1 0.5 0"
    mat(material) : "Material"
    dec(decal) : "Decal"
    scn(scene) : "Scene"
    dest(target_destination) : "Target"
    noc(target_name_or_class) : "Target or class"
    org(origin) : "Origin"
    vec(vector) : "Vector"
    vl(vecline) : "Vecline"
    sides(sidelist) : "Sides"
    part(particlesystem) : "Particles"
    npc(npcclass) : "NPC class"
    filt(filterclass) : "Filter"
    pec(pointentityclass) : "Point class"
    inst(instance_file) : "Instance"
    scr(script) : "Script"
    ro(string) readonly : "Read only" : "fixed"
    odd(ehandle) : "Unknown type"
]
"#;

pub fn test_fgd() -> ResolvedFgd {
    let fgd = Fgd::parse(TEST_FGD, None);
    fgd.resolve()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fgd_parses_cleanly() {
        let fgd = hp_fgd::Fgd::parse(super::TEST_FGD, None);
        assert!(fgd.warnings.is_empty(), "{:#?}", fgd.warnings);
        let r = fgd.resolve();
        assert!(r.warnings.is_empty(), "{:#?}", r.warnings);
        assert!(r.class("light").is_some());
        assert!(r.class("point_everything").unwrap().props.len() > 20);
    }
}
