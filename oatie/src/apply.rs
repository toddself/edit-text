//! Methods to apply an operation to a document.

use super::doc::*;
use std::collections::HashMap;
use super::wasm::*;
use crate::normalize::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "tag", content = "fields")] // Since serde(tag = "type") fails
pub enum Bytecode {
    Enter,
    Exit,
    AdvanceElements(usize),
    DeleteElements(usize),
    InsertString(String),
    WrapPrevious(usize, Attrs),
    UnwrapSelf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Program(Vec<Bytecode>);

impl Program {
    pub fn new() -> Program {
        Program(vec![])
    }

    pub fn place(&mut self, mut code: Bytecode) {
        use self::Bytecode::*;
        match (self.0.last_mut(), &mut code) {
            (Some(&mut AdvanceElements(ref mut last_n)), AdvanceElements(n)) => {
                *last_n += *n;
            }
            (Some(&mut DeleteElements(ref mut last_n)), DeleteElements(n)) => {
                *last_n += *n;
            }
            (Some(&mut InsertString(ref mut last_str)), InsertString(ref mut new_n)) => {
                *last_str = format!("{}{}", last_str.as_str(), new_n.as_str());
            }
            _ => self.0.push(code.clone()),
        }
    }

    // fn place_all(&mut self, mut codes: Vec<Bytecode>) {
    //     if codes.len() > 0 {
    //         self.place(&codes.remove(0));
    //         self.0.extend(codes.into_iter());
    //     }

    // }
}

fn apply_add_inner(bc: &mut Program, spanvec: &DocSpan, delvec: &AddSpan) -> (DocSpan, DocSpan) {
    let mut span = &spanvec[..];
    let mut del = &delvec[..];

    let mut first = None;
    if !span.is_empty() {
        first = Some(span[0].clone());
        span = &span[1..]
    }

    let mut res: DocSpan = Vec::with_capacity(span.len());

    if del.is_empty() {
        return (vec![], spanvec.clone().to_vec());
    }

    let mut d = del[0].clone();
    del = &del[1..];

    let mut exhausted = first.is_none();

    trace!("ABOUT TO APPLY ADD {:?} {:?}", first, span);

    loop {
        // Flags for whether we have partially or fully consumed an atom.
        let mut nextdel = true;
        let mut nextfirst = true;

        if exhausted {
            match d {
                AddSkip(..) | AddWithGroup(..) => {
                    panic!("exhausted document on {:?}", d);
                }
                _ => {}
            }
        }

        trace!("next {:?} {:?} {:?}", d, first, exhausted);

        match d.clone() {
            AddStyles(count, styles) => match first.clone().unwrap() {
                DocChars(mut value) => {
                    if value.char_len() < count {
                        d = AddStyles(count - value.char_len(), styles.clone());
                        value.extend_styles(&styles);
                            let inner_text = value.to_string();
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(value));
                        nextdel = false;
                    } else if value.char_len() > count {
                        let (mut left, right) = value.split_at(count);
                        left.extend_styles(&styles);
                            let inner_text = left.to_string();
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(left));
                        first = Some(DocChars(right));
                        nextfirst = false;
                    } else {
                        value.extend_styles(&styles);
                            let inner_text = value.to_string();
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(value));
                    }
                }
                DocGroup(..) => {
                    panic!("Invalid AddStyles");
                }
            },
            AddSkip(count) => match first.clone().unwrap() {
                DocChars(value) => {
                    if value.char_len() < count {
                        d = AddSkip(count - value.char_len());
                            bc.place(Bytecode::AdvanceElements(1));
                        res.place(&DocChars(value));
                        nextdel = false;
                    } else if value.char_len() > count {
                        let (left, right) = value.split_at(count);
                            let inner_text = left.to_string();
                            // we assume
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(left));
                        first = Some(DocChars(right));
                        nextfirst = false;
                    } else {
                            bc.place(Bytecode::AdvanceElements(1));
                        res.place(&DocChars(value));
                    }
                }
                DocGroup(..) => {
                    res.push(first.clone().unwrap());
                        bc.place(Bytecode::AdvanceElements(1));
                    if count > 1 {
                        d = AddSkip(count - 1);
                        nextdel = false;
                    }
                }
            },
            AddWithGroup(ref delspan) => match first.clone().unwrap() {
                DocGroup(ref attrs, ref span) => {
                        bc.place(Bytecode::Enter);
                    res.push(DocGroup(attrs.clone(), apply_add_outer(bc, span, delspan)));
                        bc.place(Bytecode::Exit);
                }
                _ => {
                    panic!("Invalid AddWithGroup");
                }
            },
            AddChars(value) => {
                    // TODO where do you skip anything, exactly
                    // need to manifest the place issue externally as well
                    let inner_text = value.to_string();
                    bc.place(Bytecode::AdvanceElements(1));
                res.place(&DocChars(value));
                nextfirst = false;
            }
            AddGroup(attrs, innerspan) => {
                let mut subdoc = vec![];
                if !exhausted {
                    subdoc.push(first.clone().unwrap());
                    subdoc.extend_from_slice(span);
                }
                trace!("CALLING INNER {:?} {:?}", subdoc, innerspan);

                let (inner, rest) = apply_add_inner(bc, &subdoc, &innerspan);
                res.place(&DocGroup(attrs.clone(), inner));

                trace!("REST OF INNER {:?} {:?}", rest, del);

                let (inner, rest) = apply_add_inner(bc, &rest, &del.to_vec());
                res.place_all(&inner);

                    // TODO not 1.
                    // Wrap previous elements in the inner span.
                    bc.place(Bytecode::WrapPrevious(1, attrs));

                return (res, rest);
            }
        }

        if nextdel {
            if del.is_empty() {
                let mut remaining = vec![];
                trace!("nextfirst {:?} {:?} {:?}", nextfirst, first, exhausted);
                if !nextfirst && first.is_some() && !exhausted {
                    remaining.push(first.clone().unwrap());
                    // place_any(&mut res, &first.clone().unwrap());
                }
                remaining.extend_from_slice(span);
                return (res, remaining);
            }

            d = del[0].clone();
            del = &del[1..];
        }

        if nextfirst {
            if span.is_empty() {
                exhausted = true;
            } else {
                first = Some(span[0].clone());
                span = &span[1..];
            }
        }
    }
}

// TODO replace all occurances of this with apply_add_inner 
fn apply_add_outer(bc: &mut Program, spanvec: &DocSpan, delvec: &AddSpan) -> DocSpan {
    let (mut res, remaining) = apply_add_inner(bc, spanvec, delvec);

    // TODO never accept unbalanced components?
    if !remaining.is_empty() {
        res.place_all(&remaining);
        // panic!("Unbalanced apply_add");
    }
    res
}

pub fn apply_add(spanvec: &DocSpan, delvec: &AddSpan) -> DocSpan {
    let mut bc = Program::new();
    let ret = apply_add_outer(&mut bc, spanvec, delvec);
    // console_log!("-------vvvv apply_add");
    // console_log!("bc: {:?}", bc);
    // console_log!("-------^^^^");
    ret
}

// TODO what does this do, why doe sit exist, for creating BC for frontend??
pub fn apply_add_bc(spanvec: &DocSpan, delvec: &AddSpan) -> Program {
    let mut bc = Program::new();
    let ret = apply_add_outer(&mut bc, spanvec, delvec);
    bc
}

fn apply_del_inner(bc: &mut Program, spanvec: &DocSpan, delvec: &DelSpan) -> DocSpan {
    let mut span = &spanvec[..];
    let mut del = &delvec[..];

    let mut res: DocSpan = Vec::with_capacity(span.len());

    if del.is_empty() {
        return span.to_vec();
    }

    let mut first = span[0].clone();
    span = &span[1..];

    let mut d = del[0].clone();
    del = &del[1..];

    loop {
        let mut nextdel = true;
        let mut nextfirst = true;

        // println!("(d) del: {:?}\n    doc: {:?}", d, first);

        match d.clone() {
            DelStyles(count, styles) => match first.clone() {
                DocChars(mut value) => {
                    if value.char_len() < count {
                        d = DelStyles(count - value.char_len(), styles.clone());
                        value.remove_styles(&styles);
                            let inner_text = value.to_string();
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(value));
                        nextdel = false;
                    } else if value.char_len() > count {
                        let (mut left, right) = value.split_at(count);
                        left.remove_styles(&styles);
                            let inner_text = left.to_string();
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(left));
                        first = DocChars(right);
                        nextfirst = false;
                    } else {
                        value.remove_styles(&styles);
                            let inner_text = value.to_string();
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(value));
                    }
                }
                _ => {
                    panic!("Invalid DelStyles");
                }
            },
            DelSkip(count) => match first.clone() {
                DocChars(value) => {
                    if value.char_len() < count {
                        d = DelSkip(count - value.char_len());
                            bc.place(Bytecode::AdvanceElements(1));
                        res.place(&DocChars(value));
                        nextdel = false;
                    } else if value.char_len() > count {
                        let (left, right) = value.split_at(count);
                            let inner_text = left.to_string();
                            // we assume
                            bc.place(Bytecode::DeleteElements(1));
                            bc.place(Bytecode::InsertString(inner_text));
                        res.place(&DocChars(left));
                        first = DocChars(right);
                        nextfirst = false;
                    } else {
                            bc.place(Bytecode::AdvanceElements(1));
                        res.place(&DocChars(value));
                        nextdel = true;
                    }
                }
                DocGroup(..) => {
                    res.push(first.clone());
                        bc.place(Bytecode::AdvanceElements(1));
                    if count > 1 {
                        d = DelSkip(count - 1);
                        nextdel = false;
                    }
                }
            },
            DelWithGroup(ref delspan) => match first.clone() {
                DocGroup(ref attrs, ref span) => {
                        bc.place(Bytecode::Enter);
                    res.push(DocGroup(attrs.clone(), apply_delete(span, delspan)));
                        bc.place(Bytecode::Exit);
                }
                _ => {
                    panic!("Invalid DelWithGroup");
                }
            },
            DelGroup(ref delspan) => match first.clone() {
                DocGroup(ref attrs, ref span) => {
                        bc.place(Bytecode::Enter);
                    res.place_all(&apply_delete(span, delspan)[..]);
                        bc.place(Bytecode::UnwrapSelf);
                }
                _ => {
                    panic!("Invalid DelGroup");
                }
            },
            DelChars(count) => match first.clone() {
                DocChars(ref value) => {
                    if value.char_len() > count {
                        let (_, right) = value.split_at(count);
                        first = DocChars(right);
                        nextfirst = false;
                    } else if value.char_len() < count {
                        d = DelChars(count - value.char_len());
                        nextdel = false;
                    } else {
                        // noop
                    }
                }
                _ => {
                    panic!("Invalid DelChars");
                }
            }, // DelObject => {
               //     unimplemented!();
               // }
               // DelMany(count) => {
               //     match first.clone() {
               //         DocChars(ref value) => {
               //             let len = value.chars().count();
               //             if len > count {
               //                 first = DocChars(value.chars().skip(count).collect());
               //                 nextfirst = false;
               //             } else if len < count {
               //                 d = DelMany(count - len);
               //                 nextdel = false;
               //             }
               //         }
               //         DocGroup(..) => {
               //             if count > 1 {
               //                 d = DelMany(count - 1);
               //                 nextdel = false;
               //             } else {
               //                 nextdel = true;
               //             }
               //         }
               //     }
               // }
               // DelGroupAll => {
               //     match first.clone() {
               //         DocGroup(..) => {}
               //         _ => {
               //             panic!("Invalid DelGroupAll");
               //         }
               //     }
               // }
        }

        if nextdel {
            if del.is_empty() {
                if !nextfirst {
                    res.place(&first)
                    // TODO res place
                }
                if !span.is_empty() {
                    res.place(&span[0]);
                    res.extend_from_slice(&span[1..]);
                }
                break;
            }

            d = del[0].clone();
            del = &del[1..];
        }

        if nextfirst {
            if span.is_empty() {
                panic!(
                    "exhausted document in apply_delete\n -->{:?}\n -->{:?}",
                    first, span
                );
            }

            first = span[0].clone();
            span = &span[1..];
        }
    }

    res
}

pub fn apply_delete(spanvec: &DocSpan, delvec: &DelSpan) -> DocSpan {
    let mut bc = Program::new();
    let ret = apply_del_inner(&mut bc, spanvec, delvec);
    // console_log!("-------vvvv apply_add");
    // console_log!("bc: {:?}", bc);
    // console_log!("-------^^^^");
    ret
}

// TODO what does this do, why doe sit exist, for creating BC for frontend??
pub fn apply_del_bc(spanvec: &DocSpan, delvec: &DelSpan) -> (DocSpan, Program) {
    let mut bc = Program::new();
    let ret = apply_del_inner(&mut bc, spanvec, delvec);
    (ret, bc)
}

pub fn apply_op_bc(spanvec: &DocSpan, op: &Op) -> Vec<Program> {
    let &(ref delvec, ref addvec) = op;
    let (postdel, del_program) = apply_del_bc(spanvec, delvec);
    let add_program = apply_add_bc(&postdel, addvec);
    vec![del_program, add_program]
}

pub fn apply_operation(spanvec: &DocSpan, op: &Op) -> DocSpan {
    let &(ref delvec, ref addvec) = op;
    // println!("------> @1 {:?}", spanvec);
    // println!("------> @2 {:?}", delvec);
    let postdel = apply_delete(spanvec, delvec);
    // println!("------> @3 {:?}", postdel);
    // println!("------> @4 {:?}", addvec);
    apply_add(&postdel, addvec)
}
