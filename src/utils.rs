use goblin::elf::program_header::PT_LOAD;
use rusb::GlobalContext;

use std::path::PathBuf;
use std::{fs::File, io::Read};

#[derive(Debug)]
pub enum UtilError {
    Elf(goblin::error::Error),
    Dfu(dfu::error::Error),
    File(std::io::Error)
}

/// Returns a contiguous bin with 0s between non-contiguous sections and starting address from an elf.
pub fn elf_to_bin(path: PathBuf) -> Result<(Vec<u8>, u32), UtilError> {
    let mut file = File::open(path).map_err(|e| UtilError::File(e))?;
    let mut buffer = vec![];
    file.read_to_end(&mut buffer).map_err(|e| UtilError::File(e))?;

    let binary = goblin::elf::Elf::parse(buffer.as_slice()).map_err(|e| UtilError::Elf(e))?;

    let mut start_address: u64 = 0;
    let mut last_address: u64 = 0;

    let mut data = vec![];
    for (i, ph) in binary
        .program_headers
        .iter()
        .filter(|ph| {
            ph.p_type == PT_LOAD
                && ph.p_filesz > 0
                && ph.p_offset >= binary.header.e_ehsize as u64
                && ph.is_read()
        })
        .enumerate()
    {
        // first time through grab the starting physical address
        if i == 0 {
            start_address = ph.p_paddr;
        }
        // on subsequent passes, if there's a gap between this section and the
        // previous one, fill it with zeros
        else {
            let difference = (ph.p_paddr - last_address) as usize;
            data.resize(data.len() + difference, 0x0);
        }

        data.extend_from_slice(&buffer[ph.p_offset as usize..][..ph.p_filesz as usize]);

        last_address = ph.p_paddr + ph.p_filesz;
    }

    Ok((data, start_address as u32))
}

pub fn flash_bin(
    binary: &[u8],
    address: u32,
    d: &rusb::Device<GlobalContext>,
) -> Result<(), UtilError> {
    let mut dfu = dfu::Dfu::from_bus_device(d.bus_number(), d.address(), 0_u32, 0_u32).map_err(|e| UtilError::Dfu(e))?;
    if binary.len() < 2048 {
        dfu.write_flash_from_slice(address, binary).map_err(|e| UtilError::Dfu(e))?;
    } else {
        // hacky bug workaround
        std::fs::write("target/out.bin", binary).map_err(|e| UtilError::File(e))?;
        dfu.download_raw(
            &mut std::fs::OpenOptions::new()
                .read(true)
                .open("target/out.bin")
                .map_err(|e| UtilError::File(e))?,
            address,
            None,
        ).map_err(|e| UtilError::Dfu(e))?;
        std::fs::remove_file("target/out.bin").map_err(|e| UtilError::File(e))?;
    }

    Ok(())
}

pub fn vendor_map() -> std::collections::HashMap<u16, Vec<u16>> {
    maplit::hashmap! {
        0x0483 => vec![0xdf11],
    }
}
