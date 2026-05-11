// From librime `key_table.h` at 08dd95f5d92 (tag v1.8.5)

#ifndef RIME_API_SYS_MODIFIERS_H
#define RIME_API_SYS_MODIFIERS_H

typedef enum {
    kShiftMask    = 1 << 0,
    kLockMask     = 1 << 1,
    kControlMask  = 1 << 2,
    kMod1Mask     = 1 << 3,
    kAltMask      = kMod1Mask,
    kMod2Mask     = 1 << 4,
    kMod3Mask     = 1 << 5,
    kMod4Mask     = 1 << 6,
    kMod5Mask     = 1 << 7,
    kButton1Mask  = 1 << 8,
    kButton2Mask  = 1 << 9,
    kButton3Mask  = 1 << 10,
    kButton4Mask  = 1 << 11,
    kButton5Mask  = 1 << 12,

    /* The next few modifiers are used by XKB, so we skip to the end.
     * Bits 15 - 23 are currently unused. Bit 29 is used internally.
     */

    /* ibus :) mask */
    kHandledMask  = 1 << 24,
    kForwardMask  = 1 << 25,
    kIgnoredMask  = kForwardMask,

    kSuperMask    = 1 << 26,
    kHyperMask    = 1 << 27,
    kMetaMask     = 1 << 28,

    kReleaseMask  = 1 << 30,

    kModifierMask = 0x5f001fff
} RimeModifier;

#endif //RIME_API_SYS_MODIFIERS_H
