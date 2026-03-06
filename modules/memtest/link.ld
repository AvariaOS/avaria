ENTRY(_module_entry)

SECTIONS {
    . = 0;

    .text : {
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.got*)
        *(.dynamic*)
        *(.dynsym*)
        *(.dynstr*)
        *(.rela*)
    }
}
