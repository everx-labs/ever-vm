/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

macro_rules! simple_commands {

    // quantity of nothing is 0
    (@count ) => { 0u8 };

    // count quantity recursively
    (@count $_x:ident = $_y:ident; $($pname:ident = $parser:ident;)*) => {
        1u8 + simple_commands!(@count $($pname = $parser;)* )
    };

    // parse command without parameters
    (@resolve $command:ident => $($code:expr),+) => {
        #[allow(non_snake_case)]
        pub fn $command(
            &mut self, 
            par: &Vec<&str>, 
            destination: &mut T
        ) -> CompileResult {
            par.assert_empty()?;
            destination.write_command(&[$($code),*])
        }  
    };

    // parse command with any parameters
    (@resolve $command:ident $($pname:ident = $parser:ident);+ => $($code:expr),+) => {
        #[allow(non_snake_case)]
        pub fn $command(
            &mut self, 
            par: &Vec<&str>, 
            destination: &mut T
        ) -> CompileResult {
            let n_params = simple_commands!(@count $($pname = $parser;)*);
            par.assert_len(n_params as usize)?;
            let mut result: Vec<u8> = vec![];
            let mut _parameters_i_:usize = 0;
            $(
                let $pname = $parser(par[_parameters_i_]).parameter("arg ".to_string() + &_parameters_i_.to_string())?;
                _parameters_i_ += 1;
            )*
            $({
                result.push($code);
            })*
            destination.write_command(result.as_slice())
        }
    };

    // parse whole block of simple commands
    ($($command: ident $($pname:ident = $parser:ident);* => $($code:expr),+ )*) => {
        $(
            simple_commands!(@resolve $command $($pname = $parser);* => $($code),*);
        )*
        pub fn enumerate_simple_commands() -> &'static [(&'static str, CompileHandler<T>)] {
            &[
                $( (stringify!($command), Engine::<T>::$command), )*
            ]
        }
    };

}