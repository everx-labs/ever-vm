/*
* Copyright (C) 2019-2024 EverX. All Rights Reserved.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific EVERX DEV software governing permissions and
* limitations under the License.
*/

use ever_block::{BuilderData, SliceData};
use super::{Stack, StackItem};

#[test]
fn test_push_increases_depth() {
    let mut stack = Stack::new();
    stack.push(StackItem::int(1));
    assert_eq!(stack.depth(), 1)
}

#[test]
fn test_take_returns_elements_from_topmost_to_bottom() {
    let mut stack = Stack::new();
    for i in 0..5 {
        stack.push(StackItem::int(i));
    }
    assert_eq!(stack.get(0), &StackItem::int(4));
    assert_eq!(stack.get(1), &StackItem::int(3));
    assert_eq!(stack.get(2), &StackItem::int(2));
}

#[test]
fn test_fift_output() {
    assert_eq!(StackItem::default().dump_as_fift(), "(null)");
    assert_eq!(StackItem::int(1200000000).dump_as_fift(), "1200000000");
    assert_eq!(StackItem::nan().dump_as_fift(), "NaN");
    let builder = BuilderData::with_bitstring(vec![0x57, 0x74]).unwrap();
    let cell = builder.clone().into_cell().unwrap();
    assert_eq!(StackItem::cell(cell.clone()).dump_as_fift(), "C{A657BCF14616E598023A10E66EA9B79E3E9CD9F93F338EB6DACE17F475A300F8}");
    assert_eq!(StackItem::builder(builder).dump_as_fift(), "BC{00035774}");
    let builder = BuilderData::with_bitstring(vec![0x57, 0x74, 0x80]).unwrap();
    assert_eq!(StackItem::builder(builder).dump_as_fift(), "BC{00045774}");
    let builder = BuilderData::with_bitstring(vec![0x57, 0x60]).unwrap();
    assert_eq!(StackItem::builder(builder).dump_as_fift(), "BC{00035760}");
    assert_eq!(StackItem::slice(SliceData::load_cell(cell).unwrap()).dump_as_fift(), "CS{Cell{00035774} bits: 0..13; refs: 0..0}");
    assert_eq!(StackItem::tuple(vec![]).dump_as_fift(), "[]");
    assert_eq!(StackItem::tuple(vec![StackItem::nan(), StackItem::int(1234567890)]).dump_as_fift(), "[ NaN 1234567890 ]");
}

mod test_serialization {
    use super::*;
    use crate::stack::continuation::ContinuationData;

    fn prepare_continuation() -> ContinuationData {
        let code = SliceData::new(vec![12, 13, 0x80]);
        let mut cont = ContinuationData::with_code(code);
        let mut item = StackItem::int(0);
        let count = if cfg!(feature="ci_run") { 1000 } else {3};
        for i in 1..count {
            item = StackItem::tuple(vec![StackItem::int(i), item]);
        }
        cont.stack.push(item);
        let tuple = vec![
            StackItem::int(888),
            StackItem::int(1234),
        ];
        cont.savelist.put(7, &mut StackItem::tuple(tuple)).unwrap();
        cont.savelist.put(4, &mut StackItem::cell(Default::default())).unwrap();
        cont.savelist.put(0, &mut StackItem::continuation(cont.clone())).unwrap();
        cont
    }

    #[test]
    fn test_continuation() {
        println!("construct");
        let cont = prepare_continuation();
        println!("serialize");
        let builder = cont.serialize(&mut 0).unwrap();
        let mut slice = SliceData::load_builder(builder).unwrap();
        println!("deserialize");
        let new_cont = ContinuationData::deserialize(&mut slice, &mut 0).unwrap();
        pretty_assertions::assert_eq!(cont, new_cont);
        println!("finish")
    }

    #[test]
    fn test_simple_item() {
        let item = StackItem::int(100500);
        let builder = item.serialize(&mut 0).unwrap();
        let slice = SliceData::load_builder(builder).unwrap();
        let new_item = StackItem::deserialize(slice, &mut 0).unwrap();
        assert_eq!(item, new_item);
    }

    #[test]
    fn test_simple_tuple() {
        let item = StackItem::tuple(vec!(
            StackItem::int(200),
            StackItem::int(100500),
        ));
        let builder = item.serialize(&mut 0).unwrap();
        let slice = SliceData::load_builder(builder).unwrap();
        let new_item = StackItem::deserialize(slice, &mut 0).unwrap();
        assert_eq!(item, new_item);
    }

    #[test]
    fn test_complex_tuple() {
        let tuple = vec!(
            StackItem::int(1),
            StackItem::int(2),
        );
        let item = StackItem::tuple(vec!(
            StackItem::int(200),
            StackItem::int(100500),
            StackItem::tuple(tuple),
        ));
        let builder = item.serialize(&mut 0).unwrap();
        let slice = SliceData::load_builder(builder).unwrap();
        let new_item = StackItem::deserialize(slice, &mut 0).unwrap();
        assert_eq!(item, new_item);
    }

    #[test]
    fn test_tuple_with_cont() {
        let continuation = prepare_continuation();
        let tuple = vec![
            StackItem::int(100500),
            StackItem::continuation(continuation),
            StackItem::int(777),
        ];
        let item = StackItem::tuple(tuple);
        let builder = item.serialize(&mut 0).unwrap();
        let slice = SliceData::load_builder(builder).unwrap();
        let new_item = StackItem::deserialize(slice, &mut 0).unwrap();
        assert_eq!(item, new_item);
    }
}
