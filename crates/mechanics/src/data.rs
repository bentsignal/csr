//! Versioned private data bundle. Only the known provider's non-executable
//! inputs are admitted. No paths or program selection come from game scripts.
use std::{collections::BTreeSet,path::{Path,PathBuf}};
use sha2::{Digest,Sha256};
const MAGIC:&[u8;8]=b"CSRDATA1";
fn allowed(path:&str)->bool{
    matches!(path,"private/game.json"|"private/skater.glb"|"private/stock/physics-skeletons.json"|"private/stock/skater-collections.json"
        |"private/stock/data/anim/OnBoard.abin"|"private/stock/data/anim/OffBoard.abin")
    || [("private/stock/data/state/","stategraph"),("private/stock/data/script/","stategraph"),
        ("private/stock/data/joystick/","pat"),("private/stock/data/camera/","shk"),("private/custom/","json")]
        .iter().any(|(prefix,extension)|path.starts_with(prefix)&&path.ends_with(&format!(".{extension}")))
}
pub fn parse(bytes:&[u8])->Result<Vec<(&str,&[u8])>,String>{
    if bytes.len()>128<<20||bytes.get(..8)!=Some(MAGIC){return Err("invalid runtime data bundle".into());}
    let mut at=8;let mut files=vec![];let mut names=BTreeSet::new();
    while at<bytes.len(){
        let size=bytes.get(at..at+6).ok_or("truncated runtime data header")?;
        let pathlen=u16::from_le_bytes(size[..2].try_into().unwrap()) as usize;
        let size=u32::from_le_bytes(size[2..].try_into().unwrap()) as usize;at+=6;
        if pathlen>240||size>64<<20||files.len()>=2048{return Err("runtime data limit exceeded".into());}
        let path=std::str::from_utf8(bytes.get(at..at+pathlen).ok_or("truncated data path")?).map_err(|_|"invalid data path")?;at+=pathlen;
        if !path.is_ascii()||path.bytes().any(|b|b<32||b"\\:<>\"|?*".contains(&b))
            ||path.split('/').any(|p|p.is_empty()||p=="."||p==".."||p.ends_with(['.',' ']))
            ||!allowed(path)||!names.insert(path.to_ascii_lowercase()){return Err("unsafe or unsupported runtime data path".into());}
        let data=bytes.get(at..at+size).ok_or("truncated runtime data file")?;at+=size;files.push((path,data));
    }
    for required in ["private/game.json","private/skater.glb","private/stock/skater-collections.json","private/stock/physics-skeletons.json",
        "private/stock/data/anim/OnBoard.abin","private/stock/data/anim/OffBoard.abin"]{
        if !files.iter().any(|(name,_)|*name==required){return Err(format!("runtime data missing {required}"));}
    }
    Ok(files)
}
pub fn build(files:&[(String,Vec<u8>)])->Result<Vec<u8>,String>{
    let mut bytes=MAGIC.to_vec();
    for (path,data) in files {bytes.extend((path.len() as u16).to_le_bytes());bytes.extend((data.len() as u32).to_le_bytes());bytes.extend(path.as_bytes());bytes.extend(data);}
    parse(&bytes)?;Ok(bytes)
}
pub fn materialize(bytes:&[u8],cache:&Path)->Result<PathBuf,String>{
    let files=parse(bytes)?;let hash=format!("{:x}",Sha256::digest(bytes));
    std::fs::create_dir_all(cache).map_err(|e|e.to_string())?;
    let cache=cache.canonicalize().map_err(|e|e.to_string())?;
    let root=cache.join(hash);std::fs::create_dir_all(&root).map_err(|e|e.to_string())?;
    let root=root.canonicalize().map_err(|e|e.to_string())?;
    if !root.starts_with(&cache){return Err("runtime cache leaves data directory".into());}
    for (path,data) in files {
        let file=root.join(path);let parent=file.parent().ok_or("invalid cache path")?;
        std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;
        if !parent.canonicalize().map_err(|e|e.to_string())?.starts_with(&root){return Err("runtime cache path leaves bundle".into());}
        if file.exists()&&!file.canonicalize().map_err(|e|e.to_string())?.starts_with(&root){return Err("runtime cache file leaves bundle".into());}
        if std::fs::read(&file).ok().as_deref()!=Some(data){std::fs::write(&file,data).map_err(|e|e.to_string())?;}
    }Ok(root)
}
