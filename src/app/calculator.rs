use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op { Add, Subtract, Multiply, Divide }
impl Op { fn symbol(self)->&'static str {match self{Self::Add=>"+",Self::Subtract=>"−",Self::Multiply=>"×",Self::Divide=>"÷"}} }

pub(super) struct CalculatorState {
    pub(super) open: bool,
    pub(super) keyboard_focus: bool,
    window_rect: Option<Rect>,
    display: String,
    expression: String,
    accumulator: Option<f64>,
    pending: Option<Op>,
    waiting: bool,
    repeat: Option<(Op, f64)>,
    memory: Option<f64>,
    error: bool,
    focus_button: usize,
}

impl Default for CalculatorState {
    fn default()->Self {Self {open:false,keyboard_focus:false,window_rect:None,display:"0".into(),expression:String::new(),accumulator:None,pending:None,waiting:false,repeat:None,memory:None,error:false,focus_button:23}}
}

const KEYPAD:[[&str;4];6]=[["%","CE","C","⌫"],["1/x","x²","√","÷"],["7","8","9","×"],["4","5","6","−"],["1","2","3","+"],["±","0",".","="]];

impl CalculatorState {
    fn value(&self) -> f64 { self.display.parse().unwrap_or(0.0) }
    fn set_value(&mut self, value: f64) {
        if !value.is_finite() { self.error = true; self.display = "エラー".into(); return; }
        self.error = false;
        self.display = format_number(value);
    }
    fn input_digit(&mut self, digit: char) {
        if self.error { self.clear_all(); }
        if self.waiting {
            self.display.clear();self.waiting=false;
            if self.pending.is_none(){self.repeat=None;self.expression.clear();self.accumulator=None;}
        }
        if self.display.len() >= 16 { return; }
        if self.display == "0" { self.display.clear(); }
        self.display.push(digit);
        if self.display.is_empty() { self.display = "0".into(); }
    }
    fn decimal(&mut self) {
        if self.error { self.clear_all(); }
        if self.waiting { self.display="0".into(); self.waiting=false; }
        if self.display.len() >= 16 { return; }
        if !self.display.contains('.') { self.display.push('.'); }
    }
    fn apply(op: Op, left: f64, right: f64) -> Option<f64> {
        let value=match op {Op::Add=>left+right,Op::Subtract=>left-right,Op::Multiply=>left*right,Op::Divide=>left/right};
        value.is_finite().then_some(value)
    }
    fn operator(&mut self, op: Op) {
        if self.error { return; }
        let current=self.value();
        if let (Some(acc),Some(pending))=(self.accumulator,self.pending) {
            if !self.waiting {
                if let Some(value)=Self::apply(pending,acc,current) { self.set_value(value); self.accumulator=Some(value); }
                else { self.error=true;self.display="エラー".into();return; }
            }
        } else { self.accumulator=Some(current); }
        self.pending=Some(op);self.waiting=true;self.repeat=None;
        self.expression=format!("{} {}",self.display,op.symbol());
    }
    fn equals(&mut self) {
        if self.error { return; }
        if let Some(op)=self.pending {
            let left=self.accumulator.unwrap_or_else(||self.value());
            let right=if self.waiting {left}else{self.value()};
            if let Some(result)=Self::apply(op,left,right) {
                self.expression=format!("{} {} {} =",format_number(left),op.symbol(),format_number(right));
                self.set_value(result);self.repeat=Some((op,right));self.accumulator=None;self.pending=None;self.waiting=true;
            } else { self.error=true;self.display="エラー".into(); }
        } else if let Some((op,right))=self.repeat {
            let left=self.value();
            if let Some(result)=Self::apply(op,left,right) { self.expression=format!("{} {} {} =",format_number(left),op.symbol(),format_number(right));self.set_value(result); }
            else {self.error=true;self.display="エラー".into();}
        }
    }
    fn percent(&mut self) {
        if self.error {return;}
        let rhs=self.value();
        let value=match (self.accumulator,self.pending) {
            (Some(left),Some(Op::Add|Op::Subtract))=>left*rhs/100.0,
            _=>rhs/100.0,
        };
        self.set_value(value);self.waiting=false;
    }
    fn unary(&mut self, action:&str) {
        if self.error{return;}
        let value=self.value();
        let result=match action {"±"=>Some(-value),"1/x"=>if value==0.0{None}else{Some(1.0/value)},"x²"=>Some(value*value),"√"=>if value<0.0{None}else{Some(value.sqrt())},_=>None};
        if let Some(result)=result {self.set_value(result);self.waiting=false;}else{self.error=true;self.display="エラー".into();}
    }
    fn clear_entry(&mut self) {if self.error{self.clear_all();}else{self.display="0".into();self.waiting=false;}}
    fn clear_all(&mut self) {self.accumulator=None;self.pending=None;self.repeat=None;self.expression.clear();self.clear_entry();}
    fn backspace(&mut self) {
        if self.error {self.clear_entry();return;}
        if self.waiting {return;}
        self.display.pop();if self.display.is_empty()||self.display=="-"{self.display="0".into();}
    }
    fn memory_action(&mut self, action:&str) {
        match action {
            "MC"=>self.memory=None,
            "MR"=>if let Some(v)=self.memory{self.set_value(v);self.waiting=false;},
            "MS"=>self.memory=Some(self.value()),
            "M+"=>self.memory=Some(self.memory.unwrap_or(0.0)+self.value()),
            "M−"=>self.memory=Some(self.memory.unwrap_or(0.0)-self.value()),
            _=>{}
        }
    }
    pub(super) fn action(&mut self, key:&str) {
        match key {
            _ if key.len()==1&&key.as_bytes()[0].is_ascii_digit()=>self.input_digit(key.chars().next().unwrap()),
            "."=>self.decimal(),
            "+"=>self.operator(Op::Add),"−"=>self.operator(Op::Subtract),
            "×"=>self.operator(Op::Multiply),"÷"=>self.operator(Op::Divide),
            "="=>self.equals(),"%"=>self.percent(),"±"|"1/x"|"x²"|"√"=>self.unary(key),
            "CE"=>self.clear_entry(),"C"=>self.clear_all(),"⌫"=>self.backspace(),
            "MC"|"MR"|"MS"|"M+"|"M−"=>self.memory_action(key),_=>{}
        }
    }
}

fn format_number(value:f64)->String {
    if value==0.0 {"0".into()} else if value.abs()>=1e16 || value.abs()<1e-12 {format!("{value:.10e}")} else {let mut s=format!("{value:.12}");while s.contains('.')&&s.ends_with('0'){s.pop();}if s.ends_with('.') {s.pop();}s}
}

impl MindmapApp {
    pub(super) fn calculator_window(&mut self,ctx:&egui::Context) {
        if !self.calculator.open{return;}
        let mut open=true;
        let mut clicked=None;
        let shown=egui::Window::new("電卓").id(egui::Id::new("calculator-window"))
            .open(&mut open).fixed_size(egui::vec2(258.0,360.0)).resizable(false)
            .show(ctx,|ui| {
                ui.set_max_height(350.0);
                ui.vertical(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(240.0,24.0),egui::Layout::bottom_up(egui::Align::RIGHT),|ui|{
                        ui.label(egui::RichText::new(&self.calculator.expression).small().color(Color32::GRAY));
                    });
                    ui.allocate_ui_with_layout(egui::vec2(240.0,42.0),egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        ui.label(egui::RichText::new(if self.calculator.display.is_empty(){"0"}else{&self.calculator.display})
                            .size(30.0).monospace().color(if self.calculator.error{Color32::from_rgb(190,35,35)}else{INK}));
                    });
                    ui.horizontal(|ui| for key in ["MC","MR","M+","M−","MS"] {
                        let enabled=key!="MR"||self.calculator.memory.is_some();
                        if ui.add_enabled(enabled,egui::Button::new(egui::RichText::new(key).size(12.0))).clicked(){clicked=Some(key);}
                    });
                    ui.add_space(3.0);
                    egui::Grid::new("calculator-keypad").spacing(egui::vec2(4.0,3.0)).show(ui,|ui|{
                        for (r,row) in KEYPAD.iter().enumerate(){
                            for (c,key) in row.iter().enumerate(){
                                let index=r*4+c;
                                let selected=index==self.calculator.focus_button;
                                let button=egui::Button::new(egui::RichText::new(*key).size(16.0))
                                    .fill(if selected{Color32::from_rgb(222,230,250)}else{ui.visuals().widgets.inactive.bg_fill});
                                if ui.add_sized(egui::vec2(52.0,32.0),button).clicked(){clicked=Some(*key);self.calculator.focus_button=index;}
                            }
                            ui.end_row();
                        }
                    });
                });
            });
        self.calculator.open=open;
        if let Some(key)=clicked { self.calculator.keyboard_focus=true;self.calculator.action(key); }
        if let Some(inner)=shown {
            let pointer=ctx.input(|i|i.pointer.interact_pos());
            self.calculator.window_rect=Some(inner.response.rect);
            if ctx.input(|i|i.pointer.any_pressed()) {
                self.calculator.keyboard_focus=pointer.is_some_and(|p|inner.response.rect.contains(p));
            }
        }
        if !open {self.calculator.keyboard_focus=false;}
    }
    pub(super) fn calculator_keys(&mut self,ctx:&egui::Context) {
        if !self.calculator.open||!self.calculator.keyboard_focus{return;}
        if ctx.input(|i|i.pointer.any_pressed()) {
            let inside=ctx.input(|i|i.pointer.interact_pos()).zip(self.calculator.window_rect)
                .is_some_and(|(p,r)|r.contains(p));
            if !inside {self.calculator.keyboard_focus=false;return;}
        }
        let mut move_focus=false;
        let action=ctx.input_mut(|i|{
            if i.modifiers.command&&i.consume_key(egui::Modifiers::COMMAND,egui::Key::C){return Some("COPY".to_owned());}
            if i.consume_key(egui::Modifiers::NONE,egui::Key::F9){return Some("±".to_owned());}
            if i.modifiers.command {
                for (key,action) in [(egui::Key::M,"MS"),(egui::Key::P,"M+"),(egui::Key::Q,"M−"),(egui::Key::R,"MR"),(egui::Key::L,"MC")] {
                    if i.consume_key(egui::Modifiers::COMMAND,key){return Some(action.to_owned());}
                }
            }
            let direct=if i.consume_key(egui::Modifiers::NONE,egui::Key::Enter){Some(KEYPAD[self.calculator.focus_button/4][self.calculator.focus_button%4].to_owned())}
                else if i.consume_key(egui::Modifiers::NONE,egui::Key::Escape){Some("C".to_owned())}
                else if i.consume_key(egui::Modifiers::NONE,egui::Key::Delete){Some("CE".to_owned())}
                else if i.consume_key(egui::Modifiers::NONE,egui::Key::Backspace){Some("⌫".to_owned())}
                else {None};
            if direct.is_some(){return direct;}
            if i.consume_key(egui::Modifiers::NONE,egui::Key::ArrowLeft){self.calculator.focus_button=self.calculator.focus_button/4*4+(self.calculator.focus_button%4+3)%4;move_focus=true;return None;}
            if i.consume_key(egui::Modifiers::NONE,egui::Key::ArrowRight){self.calculator.focus_button=self.calculator.focus_button/4*4+(self.calculator.focus_button%4+1)%4;move_focus=true;return None;}
            if i.consume_key(egui::Modifiers::NONE,egui::Key::ArrowUp){self.calculator.focus_button=(self.calculator.focus_button+20)%24;move_focus=true;return None;}
            if i.consume_key(egui::Modifiers::NONE,egui::Key::ArrowDown){self.calculator.focus_button=(self.calculator.focus_button+4)%24;move_focus=true;return None;}
            let mut text_actions=Vec::new();
            for index in (0..i.events.len()).rev(){
                if let egui::Event::Text(s)=&i.events[index]{
                    let chars:Vec<char>=s.chars().filter(|c|c.is_ascii_digit()||matches!(c,'.'|'+'|'-'|'*'|'/'|'='|'%')).collect();
                    if !chars.is_empty(){i.events.remove(index);for c in chars.into_iter().rev(){text_actions.push(match c{'*'=>"×".to_owned(),'/' =>"÷".to_owned(),'-'=>"−".to_owned(),c=>c.to_string()});}}
                }
            }
            text_actions.reverse();if text_actions.is_empty(){None}else{Some(text_actions.join("\u{1f}"))}
        });
        let changed=action.is_some()||move_focus;
        if let Some(actions)=action {for action in actions.split('\u{1f}') {if action=="COPY"{ctx.copy_text(self.calculator.display.clone());}else{self.calculator.action(action);}}}
        if changed{ctx.request_repaint();}
    }
}
