use memmap2::MmapMut;

struct JitRunner;

trait JitRunnerTrt {
    fn exec(instructions: &[u8]) -> u8;
}

impl JitRunnerTrt for JitRunner {
    fn exec(instructions: &[u8]) -> u8 {
        let mut m = MmapMut::map_anon(instructions.len()).unwrap();
        m.copy_from_slice(instructions);
        let m = m.make_exec().unwrap();
        let func_ptr = m.as_ptr();

        unsafe {
            let func: extern "C" fn() -> u8 = std::mem::transmute(func_ptr);
            func()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec() {
        let ret_code: u8 = 0x2c;
        let instructions: [u8; 6] = [
            0xb8, ret_code, 0x00, 0x00, 0x00, // mov eax, 42 (0x2a)
            0xc3, // ret
        ];
        let result = JitRunner::exec(&instructions);
        println!("Result: {:#?}", result);

        assert_eq!(result, ret_code);
    }
}
