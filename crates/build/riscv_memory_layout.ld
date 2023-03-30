MEMORY {
    RAM : ORIGIN = 0x00000000, LENGTH =  2M
}

SECTIONS {
    . = ORIGIN(RAM);
    .text : { *(.init) *(.text .text.*) } > REGION_TEXT
    .rodata : { *(.rodata) *(.rodata.*) } > REGION_RODATA
    .bss : { *(.sbss) *(.bss) *(.bss.*) } > REGION_DATA
    .data : { *(.data) } > REGION_DATA
    /DISCARD/ : { *(.eh_frame) }
    . = ALIGN(4);
}

REGION_ALIAS("REGION_TEXT", RAM);
REGION_ALIAS("REGION_RODATA", RAM);
REGION_ALIAS("REGION_DATA", RAM);
