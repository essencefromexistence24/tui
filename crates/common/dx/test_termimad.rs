use termimad::MadSkin;
fn main() { 
    let skin = MadSkin::default(); 
    let text = skin.text("Hello **world**\n\n|a|b|\n|-|-|\n|1|2|", Some(80)); 
    for (i, line) in text.lines.iter().enumerate() { 
        println!("Line {}:", i);
    } 
}
