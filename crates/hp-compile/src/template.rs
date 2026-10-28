#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens {
    pub file: String,
    pub ext: String,
    pub path: String,
    pub bspdir: String,
    pub gamedir: String,
    pub exedir: String,
    pub bsp_exe: String,
    pub vis_exe: String,
    pub light_exe: String,
    pub game_exe: String,
}

pub const TOKENS: &[&str] =
    &["$light_exe", "$game_exe", "$bsp_exe", "$vis_exe", "$gamedir", "$bspdir", "$exedir", "$path", "$file", "$ext"];

impl Tokens {
    fn value(&self, token: &str) -> &str {
        match token {
            "$file" => &self.file,
            "$ext" => &self.ext,
            "$path" => &self.path,
            "$bspdir" => &self.bspdir,
            "$gamedir" => &self.gamedir,
            "$exedir" => &self.exedir,
            "$bsp_exe" => &self.bsp_exe,
            "$vis_exe" => &self.vis_exe,
            "$light_exe" => &self.light_exe,
            "$game_exe" => &self.game_exe,
            _ => "",
        }
    }
}

pub fn expand(text: &str, tokens: &Tokens) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("$$") {
            out.push('$');
            rest = after;
            continue;
        }
        let found = TOKENS.iter().find(|t| tail.get(..t.len()).is_some_and(|s| s.eq_ignore_ascii_case(t)));
        match found {
            Some(t) => {
                out.push_str(tokens.value(t));
                rest = &tail[t.len()..];
            }
            None => {
                out.push('$');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hammer_tokens() {
        let t = Tokens {
            file: "gm_x".into(),
            ext: "vmf".into(),
            path: r"C:\maps".into(),
            bspdir: r"D:\gmod\garrysmod\maps".into(),
            gamedir: r"D:\gmod\garrysmod".into(),
            exedir: r"D:\gmod".into(),
            bsp_exe: "hammerpro.exe".into(),
            vis_exe: "hammerpro.exe".into(),
            light_exe: "hammerpro.exe".into(),
            game_exe: "gmod.exe".into(),
        };
        assert_eq!(expand(r"-game $gamedir $path\$file", &t), r"-game D:\gmod\garrysmod C:\maps\gm_x");
        assert_eq!(
            expand(r"$path\$file.bsp $bspdir\$file.bsp", &t),
            r"C:\maps\gm_x.bsp D:\gmod\garrysmod\maps\gm_x.bsp"
        );
        assert_eq!(expand("$BSP_EXE $Light_Exe", &t), "hammerpro.exe hammerpro.exe");
        assert_eq!(expand("cost $$5 $unknown $", &t), "cost $5 $unknown $");
        assert_eq!(expand("$file.$ext", &t), "gm_x.vmf");
        assert_eq!(expand("", &t), "");
        assert_eq!(expand("é$fileé", &t), "égm_xé");
        assert_eq!(expand("$é$éééé", &t), "$é$éééé");
    }
}
