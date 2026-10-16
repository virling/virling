//! One hundred flat animal faces on a 40x40 canvas, drawn to sit on a coloured circle.

pub const ANIMALS: [(&str, &str); 100] = [
    (
        "Fox",
        r##"<path d="M10 14l3-8 6 7zM30 14l-3-8-6 7z" fill="#E07A3A"/><path d="M12.5 8.5l1-2.6 2.4 3zM27.5 8.5l-1-2.6-2.4 3z" fill="#3A2A22"/><circle cx="20" cy="22" r="10" fill="#E8894A"/><path d="M10.5 23c3 0 6 2 9.5 7 3.5-5 6.5-7 9.5-7-1 5-5 9-9.5 9s-8.5-4-9.5-9z" fill="#FFF4EA"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="26.5" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Owl",
        r##"<path d="M11 12l2 4M29 12l-2 4" stroke="#7A5233" stroke-width="2.6" stroke-linecap="round"/><circle cx="20" cy="23" r="11" fill="#9C6B43"/><circle cx="15.5" cy="21" r="4.4" fill="#FFF4E0"/><circle cx="24.5" cy="21" r="4.4" fill="#FFF4E0"/><circle cx="15.5" cy="21" r="2" fill="#1F1A17"/><circle cx="24.5" cy="21" r="2" fill="#1F1A17"/><path d="M18.5 25l1.5 3 1.5-3z" fill="#E8A13A"/><path d="M14 30q6 3 12 0" stroke="#7A5233" stroke-width="1" fill="none"/>"##,
    ),
    (
        "Panda",
        r##"<circle cx="12" cy="13" r="3.8" fill="#24201D"/><circle cx="28" cy="13" r="3.8" fill="#24201D"/><circle cx="20" cy="22" r="10" fill="#FAFAF7"/><ellipse cx="15.5" cy="21" rx="2.8" ry="3.5" fill="#24201D" transform="rotate(-25 15.5 21)"/><ellipse cx="24.5" cy="21" rx="2.8" ry="3.5" fill="#24201D" transform="rotate(25 24.5 21)"/><circle cx="16" cy="20.5" r="0.9" fill="#FFF"/><circle cx="24" cy="20.5" r="0.9" fill="#FFF"/><ellipse cx="20" cy="26" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Koala",
        r##"<circle cx="10.5" cy="17" r="5.5" fill="#9AA0A6"/><circle cx="29.5" cy="17" r="5.5" fill="#9AA0A6"/><circle cx="10.5" cy="17" r="3" fill="#E9D8DC"/><circle cx="29.5" cy="17" r="3" fill="#E9D8DC"/><circle cx="20" cy="22" r="10" fill="#AEB4BA"/><circle cx="15.5" cy="20.5" r="1.3" fill="#1F1A17"/><circle cx="24.5" cy="20.5" r="1.3" fill="#1F1A17"/><ellipse cx="20" cy="25" rx="2.8" ry="3.4" fill="#3A3633"/>"##,
    ),
    (
        "Bunny",
        r##"<rect x="13" y="2" width="5" height="14" rx="2.5" fill="#D9CFC4"/><rect x="22" y="2" width="5" height="14" rx="2.5" fill="#D9CFC4"/><rect x="14.4" y="4" width="2.2" height="10" rx="1.1" fill="#F0B9C2"/><rect x="23.4" y="4" width="2.2" height="10" rx="1.1" fill="#F0B9C2"/><circle cx="20" cy="22" r="10" fill="#E6DED5"/><circle cx="16" cy="21" r="1.4" fill="#1F1A17"/><circle cx="24" cy="21" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="25.5" rx="1.4" ry="1" fill="#E28A9A"/><g stroke="#6B5E57" stroke-width=".7" stroke-linecap="round"><path d="M13 26h-4M13 27.5l-3.5 1M27 26h4M27 27.5l3.5 1"/></g>"##,
    ),
    (
        "Otter",
        r##"<circle cx="11.5" cy="17" r="2.2" fill="#5C3E2B"/><circle cx="28.5" cy="17" r="2.2" fill="#5C3E2B"/><ellipse cx="20" cy="22.5" rx="10.5" ry="9.5" fill="#6B4630"/><path d="M11 21c2-3 5.5-4 9-4s7 1 9 4c0 5-4 10-9 10s-9-5-9-10z" fill="#D9BFA0"/><circle cx="15.8" cy="21" r="1.3" fill="#1F1A17"/><circle cx="24.2" cy="21" r="1.3" fill="#1F1A17"/><path d="M17.8 24.4q2.2-1.2 4.4 0-.6 1.8-2.2 1.8t-2.2-1.8z" fill="#1F1A17"/><g stroke="#4A3324" stroke-width=".7" stroke-linecap="round"><path d="M13 26.5h-4M13 28.0l-3.5 1M27 26.5h4M27 28.0l3.5 1"/></g>"##,
    ),
    (
        "Bear",
        r##"<circle cx="12" cy="14" r="3.6" fill="#94592A"/><circle cx="28" cy="14" r="3.6" fill="#94592A"/><circle cx="12" cy="14" r="1.8" fill="#E8C79A"/><circle cx="28" cy="14" r="1.8" fill="#E8C79A"/><circle cx="20" cy="22" r="10" fill="#A86B35"/><ellipse cx="20" cy="26" rx="4.8" ry="3.8" fill="#E8C79A"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="24.8" rx="1.9" ry="1.3" fill="#1F1A17"/>"##,
    ),
    (
        "Cat",
        r##"<path d="M10.5 16l1-9 6.5 5zM29.5 16l-1-9-6.5 5z" fill="#8E949B"/><path d="M12 13l.6-4 3 2.4zM28 13l-.6-4-3 2.4z" fill="#E8B9C1"/><circle cx="20" cy="22" r="10" fill="#9DA3AA"/><path d="M17 19l-1-2.5M20 18.5v-3M23 19l1-2.5" stroke="#6F757C" stroke-width="1" stroke-linecap="round"/><ellipse cx="16" cy="22" rx="1.3" ry="1.8" fill="#1F1A17"/><ellipse cx="24" cy="22" rx="1.3" ry="1.8" fill="#1F1A17"/><ellipse cx="20" cy="25.5" rx="1.2" ry="0.9" fill="#E28A9A"/><g stroke="#6B5E57" stroke-width=".7" stroke-linecap="round"><path d="M13 26.5h-4M13 28.0l-3.5 1M27 26.5h4M27 28.0l3.5 1"/></g>"##,
    ),
    (
        "Tiger",
        r##"<circle cx="12" cy="14" r="3.2" fill="#E8893A"/><circle cx="28" cy="14" r="3.2" fill="#E8893A"/><circle cx="20" cy="22" r="10" fill="#EF9B4A"/><path d="M20 12v4M16 13l1 3M24 13l-1 3M11 21h3M26 21h3" stroke="#3A2A22" stroke-width="1.4" stroke-linecap="round"/><ellipse cx="20" cy="26.5" rx="5.5" ry="4" fill="#FFF4EA"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="25" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Lion",
        r##"<circle cx="20" cy="22" r="14" fill="#C2702E"/><circle cx="20" cy="22" r="9.5" fill="#E8B064"/><ellipse cx="20" cy="26" rx="4.5" ry="3.4" fill="#F6DDB0"/><circle cx="16.2" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="23.8" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="24.6" rx="1.8" ry="1.2" fill="#6B3A1E"/>"##,
    ),
    (
        "Frog",
        r##"<circle cx="14" cy="14" r="4.4" fill="#6FB46A"/><circle cx="26" cy="14" r="4.4" fill="#6FB46A"/><circle cx="14" cy="14" r="2.6" fill="#FFF"/><circle cx="26" cy="14" r="2.6" fill="#FFF"/><circle cx="14" cy="14.3" r="1.3" fill="#1F1A17"/><circle cx="26" cy="14.3" r="1.3" fill="#1F1A17"/><ellipse cx="20" cy="24" rx="11" ry="8.5" fill="#7CC276"/><path d="M13.5 25q6.5 5 13 0" stroke="#2F5E2C" stroke-width="1.3" fill="none" stroke-linecap="round"/><circle cx="18.5" cy="22" r=".6" fill="#2F5E2C"/><circle cx="21.5" cy="22" r=".6" fill="#2F5E2C"/>"##,
    ),
    (
        "Mouse",
        r##"<circle cx="11" cy="14" r="5.4" fill="#A6A9AE"/><circle cx="29" cy="14" r="5.4" fill="#A6A9AE"/><circle cx="11" cy="14" r="3.2" fill="#EDC3CA"/><circle cx="29" cy="14" r="3.2" fill="#EDC3CA"/><circle cx="20" cy="23" r="9.5" fill="#B7BABF"/><circle cx="16.4" cy="22" r="1.4" fill="#1F1A17"/><circle cx="23.6" cy="22" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="26.5" rx="1.3" ry="1" fill="#E28A9A"/><g stroke="#6B5E57" stroke-width=".7" stroke-linecap="round"><path d="M13 27h-4M13 28.5l-3.5 1M27 27h4M27 28.5l3.5 1"/></g>"##,
    ),
    (
        "Pig",
        r##"<path d="M11 15l2-6 5 4zM29 15l-2-6-5 4z" fill="#EBA3AE"/><circle cx="20" cy="22" r="10" fill="#F4B5BE"/><ellipse cx="20" cy="25.5" rx="4.6" ry="3.4" fill="#E68E9C"/><ellipse cx="18.3" cy="25.5" rx=".9" ry="1.3" fill="#9C4756"/><ellipse cx="21.7" cy="25.5" rx=".9" ry="1.3" fill="#9C4756"/><circle cx="16" cy="19.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="19.5" r="1.4" fill="#1F1A17"/>"##,
    ),
    (
        "Cow",
        r##"<path d="M12 12q-3-4-1-6M28 12q3-4 1-6" stroke="#CDB59A" stroke-width="2" fill="none" stroke-linecap="round"/><ellipse cx="9" cy="18" rx="3.6" ry="2" fill="#F4F1EC" transform="rotate(-20 9 18)"/><ellipse cx="31" cy="18" rx="3.6" ry="2" fill="#F4F1EC" transform="rotate(20 31 18)"/><circle cx="20" cy="22" r="10" fill="#F7F4EF"/><path d="M22 13c4 0 7 2 7 6-3 0-5-2-7-6z" fill="#6B4A36"/><ellipse cx="20" cy="27" rx="6.4" ry="4.4" fill="#F2B8BF"/><circle cx="17.6" cy="27" r=".9" fill="#9C4756"/><circle cx="22.4" cy="27" r=".9" fill="#9C4756"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/>"##,
    ),
    (
        "Sheep",
        r##"<g fill="#F7F5F0"><circle cx="13" cy="14" r="4.5"/><circle cx="20" cy="11.5" r="5"/><circle cx="27" cy="14" r="4.5"/><circle cx="11" cy="21" r="4"/><circle cx="29" cy="21" r="4"/></g><ellipse cx="20" cy="24" rx="7" ry="8" fill="#5E5651"/><ellipse cx="11.5" cy="22" rx="3" ry="1.6" fill="#5E5651" transform="rotate(-25 11.5 22)"/><ellipse cx="28.5" cy="22" rx="3" ry="1.6" fill="#5E5651" transform="rotate(25 28.5 22)"/><circle cx="17" cy="22" r="1.2" fill="#FFF"/><circle cx="23" cy="22" r="1.2" fill="#FFF"/><ellipse cx="20" cy="27.5" rx="1.4" ry="0.8" fill="#2E2825"/>"##,
    ),
    (
        "Deer",
        r##"<path d="M14 13l-3-6m1.5 3l-3 .5M26 13l3-6m-1.5 3l3 .5" stroke="#7A5236" stroke-width="1.6" stroke-linecap="round" fill="none"/><ellipse cx="10" cy="18" rx="3.6" ry="2" fill="#B07A4C" transform="rotate(-25 10 18)"/><ellipse cx="30" cy="18" rx="3.6" ry="2" fill="#B07A4C" transform="rotate(25 30 18)"/><ellipse cx="20" cy="23" rx="8.5" ry="10" fill="#C08A58"/><ellipse cx="20" cy="28" rx="4" ry="3.4" fill="#EAD2B6"/><circle cx="16.4" cy="21" r="1.4" fill="#1F1A17"/><circle cx="23.6" cy="21" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="27" rx="1.6" ry="1.1" fill="#1F1A17"/>"##,
    ),
    (
        "Penguin",
        r##"<circle cx="20" cy="22" r="11" fill="#26292E"/><path d="M20 15c-2.5-2.2-7.5-1.3-7.5 3.5 0 5 4 9 7.5 12 3.5-3 7.5-7 7.5-12 0-4.8-5-5.7-7.5-3.5z" fill="#FAFAF7"/><circle cx="16.6" cy="20.5" r="1.3" fill="#1F1A17"/><circle cx="23.4" cy="20.5" r="1.3" fill="#1F1A17"/><path d="M17.8 24h4.4L20 27.5z" fill="#F0A33A"/>"##,
    ),
    (
        "Duck",
        r##"<circle cx="20" cy="22" r="11" fill="#F4C84A"/><circle cx="15.8" cy="19.5" r="1.4" fill="#1F1A17"/><circle cx="24.2" cy="19.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="26" rx="6.5" ry="2.8" fill="#EE8F2E"/><path d="M14 25.6h12" stroke="#C26A1A" stroke-width=".8"/>"##,
    ),
    (
        "Chick",
        r##"<path d="M19 11.5q1-4 3-3M20 11.5q-2-3.5-4-2" stroke="#E2A72E" stroke-width="1.4" fill="none" stroke-linecap="round"/><circle cx="20" cy="22" r="10.5" fill="#FBD85A"/><circle cx="16.4" cy="21" r="1.4" fill="#1F1A17"/><circle cx="23.6" cy="21" r="1.4" fill="#1F1A17"/><path d="M18 24.5h4l-2 2.6z" fill="#EE8F2E"/><circle cx="13.5" cy="25" r="1.6" fill="#F6A99A" opacity=".7"/><circle cx="26.5" cy="25" r="1.6" fill="#F6A99A" opacity=".7"/>"##,
    ),
    (
        "Monkey",
        r##"<circle cx="9.5" cy="21" r="4" fill="#8A5A3A"/><circle cx="30.5" cy="21" r="4" fill="#8A5A3A"/><circle cx="9.5" cy="21" r="2.2" fill="#E6C29C"/><circle cx="30.5" cy="21" r="2.2" fill="#E6C29C"/><circle cx="20" cy="22" r="10" fill="#8A5A3A"/><path d="M20 15c-3-2-7.5 0-7 4 .3 2 1.5 3 1.5 4 0 4 2.5 7 5.5 7s5.5-3 5.5-7c0-1 1.2-2 1.5-4 .5-4-4-6-7-4z" fill="#E6C29C"/><circle cx="17" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="23" cy="20.5" r="1.4" fill="#1F1A17"/><path d="M17.5 26.5q2.5 2 5 0" stroke="#1F1A17" stroke-width="1.1" fill="none" stroke-linecap="round"/><circle cx="19" cy="24" r=".5" fill="#1F1A17"/><circle cx="21" cy="24" r=".5" fill="#1F1A17"/>"##,
    ),
    (
        "Raccoon",
        r##"<path d="M10.5 16l1.5-8 6 5zM29.5 16l-1.5-8-6 5z" fill="#7D8086"/><circle cx="20" cy="22" r="10" fill="#9DA0A6"/><path d="M9.5 21c2-3 6-3.5 10.5-1.5 4.5-2 8.5-1.5 10.5 1.5-2 2.5-5.5 3-10.5 2-5 1-8.5.5-10.5-2z" fill="#2E2B29"/><ellipse cx="20" cy="27" rx="4.6" ry="3.4" fill="#F2EFEA"/><circle cx="15.8" cy="21" r="1.2" fill="#FFF"/><circle cx="24.2" cy="21" r="1.2" fill="#FFF"/><ellipse cx="20" cy="25.6" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Badger",
        r##"<circle cx="20" cy="22" r="10" fill="#2E2B29"/><path d="M18 12h4v16h-4z" fill="#F4F1EC"/><path d="M10.5 24c2-1 4-1 6 .5-1 3-3 4.5-5.5 4-.5-1.5-.7-3-.5-4.5zM29.5 24c-2-1-4-1-6 .5 1 3 3 4.5 5.5 4 .5-1.5.7-3 .5-4.5z" fill="#F4F1EC"/><circle cx="15.6" cy="20.5" r="1.3" fill="#FFF"/><circle cx="24.4" cy="20.5" r="1.3" fill="#FFF"/><ellipse cx="20" cy="27.5" rx="1.6" ry="1" fill="#E9E4DD"/>"##,
    ),
    (
        "Beaver",
        r##"<circle cx="12" cy="14.5" r="2.6" fill="#5A3A26"/><circle cx="28" cy="14.5" r="2.6" fill="#5A3A26"/><circle cx="20" cy="22" r="10" fill="#7A5034"/><ellipse cx="20" cy="25" rx="5.4" ry="3.6" fill="#C69A72"/><circle cx="16" cy="20" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="23.8" rx="1.8" ry="1.2" fill="#1F1A17"/><rect x="17.6" y="26" width="4.8" height="5" rx=".8" fill="#FFFDF5" stroke="#B8A58F" stroke-width=".6"/><path d="M20 26.2v4.6" stroke="#B8A58F" stroke-width=".6"/>"##,
    ),
    (
        "Hedgehog",
        r##"<path d="M6 26l2-5-3-3 4-2-2-4 5 0 0-4 4 2 2-4 3 3 3-3 2 4 4-2 0 4 5 0-2 4 4 2-3 3 2 5z" fill="#7A5A44"/><ellipse cx="20" cy="25" rx="8" ry="7" fill="#EAD7C1"/><circle cx="17" cy="23.5" r="1.2" fill="#1F1A17"/><circle cx="23" cy="23.5" r="1.2" fill="#1F1A17"/><ellipse cx="20" cy="27.5" rx="1.4" ry="1" fill="#1F1A17"/>"##,
    ),
    (
        "Bat",
        r##"<path d="M8 22c-2-8 1-13 4-15 1 4 3 6 6 7M32 22c2-8-1-13-4-15-1 4-3 6-6 7" fill="#5A4E6E"/><circle cx="20" cy="23" r="9.5" fill="#6A5D80"/><circle cx="16.4" cy="22" r="1.3" fill="#FFF"/><circle cx="23.6" cy="22" r="1.3" fill="#FFF"/><circle cx="16.4" cy="22" r=".6" fill="#1F1A17"/><circle cx="23.6" cy="22" r=".6" fill="#1F1A17"/><path d="M18 27l.8 1.6.8-1.6M20.4 27l.8 1.6.8-1.6" fill="#FFF"/>"##,
    ),
    (
        "Squirrel",
        r##"<path d="M30 38c8-2 9-14 2-20-3-2.5-6-1-5 2 4 2 4 9-1 12z" fill="#C46A2E"/><path d="M12 15l1-6 4 4zM28 15l-1-6-4 4z" fill="#B85E27"/><circle cx="20" cy="22" r="9.5" fill="#D07636"/><ellipse cx="20" cy="26" rx="4.4" ry="3.6" fill="#F4D9BC"/><circle cx="16.4" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="23.6" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="24.6" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Wolf",
        r##"<path d="M10.5 17l1-10 7 5.5zM29.5 17l-1-10-7 5.5z" fill="#7F858C"/><circle cx="20" cy="22" r="10" fill="#959BA2"/><path d="M12 23c3 0 6 2 8 6 2-4 5-6 8-6-1 5-4 8-8 8s-7-3-8-8z" fill="#EEF0F2"/><circle cx="16" cy="20.5" r="1.2" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.2" fill="#1F1A17"/><ellipse cx="20" cy="26" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Seal",
        r##"<circle cx="20" cy="23" r="11" fill="#9AA6B2"/><ellipse cx="20" cy="26.5" rx="5.6" ry="3.8" fill="#C4CDD6"/><circle cx="15.6" cy="20.5" r="1.6" fill="#1F1A17"/><circle cx="24.4" cy="20.5" r="1.6" fill="#1F1A17"/><ellipse cx="20" cy="24.6" rx="1.8" ry="1.2" fill="#1F1A17"/><circle cx="17" cy="27" r=".5" fill="#5D6873"/><circle cx="23" cy="27" r=".5" fill="#5D6873"/><circle cx="16.5" cy="28.5" r=".5" fill="#5D6873"/><circle cx="23.5" cy="28.5" r=".5" fill="#5D6873"/>"##,
    ),
    (
        "Raven",
        r##"<path d="M19 12q2-4 5-4-1 2 0 4" fill="#2A2C33"/><circle cx="20" cy="22" r="10.5" fill="#30333B"/><circle cx="16" cy="21" r="1.6" fill="#FFF"/><circle cx="24" cy="21" r="1.6" fill="#FFF"/><circle cx="16" cy="21" r=".8" fill="#1F1A17"/><circle cx="24" cy="21" r=".8" fill="#1F1A17"/><path d="M17 24l3 6 3-6z" fill="#6E7480"/>"##,
    ),
    (
        "Robin",
        r##"<circle cx="20" cy="22" r="10.5" fill="#8A6448"/><path d="M12 28c1.5-2.5 4.5-3.5 8-3.5s6.5 1 8 3.5c-1.8 3-4.6 4.4-8 4.4s-6.2-1.4-8-4.4z" fill="#E36A3A"/><circle cx="16.2" cy="19" r="1.4" fill="#1F1A17"/><circle cx="23.8" cy="19" r="1.4" fill="#1F1A17"/><path d="M18.4 21.8h3.2L20 24.4z" fill="#3A2A22"/>"##,
    ),
    (
        "Puffin",
        r##"<circle cx="20" cy="22" r="11" fill="#26292E"/><path d="M20 14c-4 0-8 3-8 7.5 0 2.5 1.5 4 3 4.5h10c1.5-.5 3-2 3-4.5 0-4.5-4-7.5-8-7.5z" fill="#F7F5F0"/><circle cx="16" cy="20.5" r="1.2" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.2" fill="#1F1A17"/><path d="M16 24.5q4-2 8 0-1 5-4 6.5-3-1.5-4-6.5z" fill="#EE7A30"/><path d="M16.8 26.5q3.2-1.6 6.4 0" stroke="#F6C04A" stroke-width="1.2" fill="none"/>"##,
    ),
    (
        "Swan",
        r##"<path d="M15 40c0-9 3-12 3-17 0-4-2-6-2-9 0-3 2.5-5 5.5-5S27 11 27 14" stroke="#FAFAF7" stroke-width="6" fill="none" stroke-linecap="round"/><path d="M26.5 13.5l6 2.2-6 1.6z" fill="#EE8F2E"/><path d="M26 12.4l1.8 1.4-1.8 1z" fill="#1F1A17"/><circle cx="23.2" cy="12.6" r="1.1" fill="#1F1A17"/>"##,
    ),
    (
        "Elephant",
        r##"<ellipse cx="9" cy="21" rx="6" ry="7.5" fill="#9AA3AD"/><ellipse cx="31" cy="21" rx="6" ry="7.5" fill="#9AA3AD"/><ellipse cx="9" cy="21" rx="3.6" ry="5" fill="#E8C9CF"/><ellipse cx="31" cy="21" rx="3.6" ry="5" fill="#E8C9CF"/><circle cx="20" cy="21" r="9.5" fill="#AAB3BC"/><path d="M18 25c0 5 0 8 2 11 1.5 1 3 0 2.5-1.5-1.5-3-1.5-6-1.5-9.5z" fill="#AAB3BC"/><circle cx="16.4" cy="19.5" r="1.2" fill="#1F1A17"/><circle cx="23.6" cy="19.5" r="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Giraffe",
        r##"<path d="M15 10l-1-5M25 10l1-5" stroke="#B87A3A" stroke-width="1.6" stroke-linecap="round"/><circle cx="14" cy="5" r="1.4" fill="#7A4A22"/><circle cx="26" cy="5" r="1.4" fill="#7A4A22"/><ellipse cx="10" cy="16" rx="3.4" ry="1.8" fill="#E8B456" transform="rotate(-25 10 16)"/><ellipse cx="30" cy="16" rx="3.4" ry="1.8" fill="#E8B456" transform="rotate(25 30 16)"/><ellipse cx="20" cy="21" rx="8" ry="11" fill="#F0C060"/><g fill="#C27A30"><circle cx="15" cy="15" r="1.6"/><circle cx="25" cy="14" r="1.4"/><circle cx="14" cy="23" r="1.2"/><circle cx="26.5" cy="22" r="1.5"/></g><ellipse cx="20" cy="28" rx="5" ry="4" fill="#F6DDA8"/><circle cx="16.4" cy="19.5" r="1.4" fill="#1F1A17"/><circle cx="23.6" cy="19.5" r="1.4" fill="#1F1A17"/><circle cx="18" cy="28" r=".7" fill="#7A4A22"/><circle cx="22" cy="28" r=".7" fill="#7A4A22"/>"##,
    ),
    (
        "Zebra",
        r##"<path d="M12 13l1-7 4 6zM28 13l-1-7-4 6z" fill="#F4F3F0"/><ellipse cx="20" cy="22" rx="8.5" ry="11" fill="#F4F3F0"/><path d="M14 13l3 3M13 18l4 1.5M26 13l-3 3M27 18l-4 1.5M20 11v4" stroke="#24201D" stroke-width="1.6" stroke-linecap="round"/><path d="M18 9h4l-1 4h-2z" fill="#24201D"/><ellipse cx="20" cy="28.5" rx="5.6" ry="4.4" fill="#3A3633"/><circle cx="16" cy="21" r="1.2" fill="#1F1A17"/><circle cx="24" cy="21" r="1.2" fill="#1F1A17"/><circle cx="18" cy="29" r=".7" fill="#999"/><circle cx="22" cy="29" r=".7" fill="#999"/>"##,
    ),
    (
        "Flamingo",
        r##"<path d="M20 40c-1-8 3-12 3-17" stroke="#F49AB0" stroke-width="5" fill="none" stroke-linecap="round"/><circle cx="20" cy="17" r="8" fill="#F7A9BB"/><circle cx="17" cy="15.5" r="1.1" fill="#1F1A17"/><circle cx="23" cy="15.5" r="1.1" fill="#1F1A17"/><path d="M22 18q6 1 6 5-2 2-4 0-1-2-2-2z" fill="#F7F3EE"/><path d="M26.3 21.5q1.7 1.5 1.7 1.5-2 2-4 0z" fill="#24201D"/>"##,
    ),
    (
        "Hippo",
        r##"<circle cx="12" cy="12" r="2.6" fill="#8E7F9C"/><circle cx="28" cy="12" r="2.6" fill="#8E7F9C"/><ellipse cx="20" cy="19" rx="9" ry="7" fill="#9E8FAC"/><ellipse cx="20" cy="28" rx="11" ry="7" fill="#B2A4BF"/><circle cx="16.4" cy="17" r="1.2" fill="#1F1A17"/><circle cx="23.6" cy="17" r="1.2" fill="#1F1A17"/><ellipse cx="16.5" cy="27" rx="1.2" ry=".9" fill="#5E4F6C"/><ellipse cx="23.5" cy="27" rx="1.2" ry=".9" fill="#5E4F6C"/>"##,
    ),
    (
        "Octopus",
        r##"<path d="M10 28q-2 6-5 6M14 30q0 6-3 8M20 31v8M26 30q0 6 3 8M30 28q2 6 5 6" stroke="#B967A8" stroke-width="3" fill="none" stroke-linecap="round"/><ellipse cx="20" cy="20" rx="11" ry="11" fill="#C977B8"/><circle cx="16" cy="21" r="1.6" fill="#1F1A17"/><circle cx="24" cy="21" r="1.6" fill="#1F1A17"/><path d="M17.5 25.5q2.5 2 5 0" stroke="#1F1A17" stroke-width="1.1" fill="none" stroke-linecap="round"/><circle cx="14" cy="14" r="1.2" fill="#DC9BCD"/><circle cx="26" cy="13" r="1" fill="#DC9BCD"/>"##,
    ),
    (
        "Parrot",
        r##"<path d="M14 9q4-5 8-2-3 1-3 4" fill="#3BA55C"/><circle cx="20" cy="22" r="10.5" fill="#43B866"/><path d="M12 22c0-4 3-7 8-7 0 6-3 9-8 9z" fill="#F4D34A"/><circle cx="17" cy="19" r="2.4" fill="#FFF"/><circle cx="17" cy="19" r="1.2" fill="#1F1A17"/><path d="M20 21q8 0 7 8-2-2-4-2-3-2-3-6z" fill="#3A3633"/>"##,
    ),
    (
        "Turtle",
        r##"<ellipse cx="20" cy="27" rx="13" ry="9" fill="#5E9B5A"/><path d="M14 22l3 4-3 4M26 22l-3 4 3 4M17 26h6" stroke="#4A7E47" stroke-width="1.2" fill="none"/><circle cx="20" cy="15" r="7.5" fill="#8CC38A"/><circle cx="17" cy="14.5" r="1.2" fill="#1F1A17"/><circle cx="23" cy="14.5" r="1.2" fill="#1F1A17"/><path d="M18 18.5q2 1.4 4 0" stroke="#1F1A17" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Whale",
        r##"<path d="M18 9q-1-4 2-5 3 1 2 5" fill="#9CD3F0"/><ellipse cx="20" cy="23" rx="12" ry="10" fill="#4C86C6"/><path d="M8 25q12 9 24 0-2 7-12 7S10 32 8 25z" fill="#DCE9F5"/><circle cx="15.5" cy="20" r="1.3" fill="#1F1A17"/><circle cx="24.5" cy="20" r="1.3" fill="#1F1A17"/><path d="M17 25.5q3 1.6 6 0" stroke="#1F1A17" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Camel",
        r##"<ellipse cx="11" cy="15" rx="2.6" ry="1.6" fill="#C9A06A" transform="rotate(-30 11 15)"/><ellipse cx="29" cy="15" rx="2.6" ry="1.6" fill="#C9A06A" transform="rotate(30 29 15)"/><ellipse cx="20" cy="21" rx="8.5" ry="10.5" fill="#D7AE76"/><path d="M14 13q6-4 12 0" stroke="#B08452" stroke-width="2" fill="none" stroke-linecap="round"/><ellipse cx="20" cy="27.5" rx="5.6" ry="4.4" fill="#E8C99A"/><circle cx="16.4" cy="20" r="1.2" fill="#1F1A17"/><circle cx="23.6" cy="20" r="1.2" fill="#1F1A17"/><path d="M17.5 27h1.5M21 27h1.5M18 30q2 1 4 0" stroke="#7A5A34" stroke-width=".9" stroke-linecap="round" fill="none"/>"##,
    ),
    (
        "Kangaroo",
        r##"<ellipse cx="13" cy="9" rx="2.4" ry="5.5" fill="#B87A48" transform="rotate(-15 13 9)"/><ellipse cx="27" cy="9" rx="2.4" ry="5.5" fill="#B87A48" transform="rotate(15 27 9)"/><ellipse cx="20" cy="21" rx="8.5" ry="9.5" fill="#C98A55"/><ellipse cx="20" cy="27" rx="5" ry="4" fill="#E9C6A0"/><circle cx="16.4" cy="19.5" r="1.4" fill="#1F1A17"/><circle cx="23.6" cy="19.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="25.4" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Peacock",
        r##"<g fill="#2FA88C"><ellipse cx="8" cy="18" rx="4" ry="5.5" transform="rotate(-50 8 18)"/><ellipse cx="12" cy="10" rx="4" ry="5.5" transform="rotate(-25 12 10)"/><ellipse cx="20" cy="7" rx="4" ry="5.5"/><ellipse cx="28" cy="10" rx="4" ry="5.5" transform="rotate(25 28 10)"/><ellipse cx="32" cy="18" rx="4" ry="5.5" transform="rotate(50 32 18)"/></g><g fill="#2D5DA8"><circle cx="7.5" cy="17" r="1.6"/><circle cx="11.5" cy="9" r="1.6"/><circle cx="20" cy="5.5" r="1.6"/><circle cx="28.5" cy="9" r="1.6"/><circle cx="32.5" cy="17" r="1.6"/></g><path d="M19 13l1-3 1 3" stroke="#2D5DA8" stroke-width="1"/><circle cx="20" cy="24" r="8.5" fill="#2D7BC4"/><path d="M15.5 21q2-1.5 3.5 0M21 21q2-1.5 3.5 0" stroke="#FFF" stroke-width="1.2" fill="none"/><circle cx="16.8" cy="22.5" r="1.2" fill="#1F1A17"/><circle cx="23.2" cy="22.5" r="1.2" fill="#1F1A17"/><path d="M18.6 26h2.8L20 28.6z" fill="#E8A13A"/>"##,
    ),
    (
        "Rhino",
        r##"<circle cx="12" cy="13" r="2.4" fill="#8F9399"/><circle cx="28" cy="13" r="2.4" fill="#8F9399"/><circle cx="20" cy="22" r="10" fill="#A3A7AD"/><path d="M18 22l2-9 2 9z" fill="#E7E2D8"/><circle cx="15.4" cy="21" r="1.2" fill="#1F1A17"/><circle cx="24.6" cy="21" r="1.2" fill="#1F1A17"/><ellipse cx="20" cy="28" rx="5.6" ry="3.6" fill="#B9BDC2"/>"##,
    ),
    (
        "Crab",
        r##"<path d="M8 18q-5-3-3-8l4 2zM32 18q5-3 3-8l-4 2z" fill="#E2553A"/><path d="M12 30l-5 4M14 32l-3 5M28 30l5 4M26 32l3 5" stroke="#E2553A" stroke-width="2" stroke-linecap="round"/><ellipse cx="20" cy="25" rx="11" ry="8" fill="#EE6A48"/><path d="M16 17v-4M24 17v-4" stroke="#EE6A48" stroke-width="1.6"/><circle cx="16" cy="13" r="2" fill="#FFF"/><circle cx="24" cy="13" r="2" fill="#FFF"/><circle cx="16" cy="13" r="1" fill="#1F1A17"/><circle cx="24" cy="13" r="1" fill="#1F1A17"/><path d="M17 27q3 2 6 0" stroke="#1F1A17" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Bee",
        r##"<ellipse cx="13" cy="12" rx="5" ry="4" fill="#EAF4FB" opacity=".9"/><ellipse cx="27" cy="12" rx="5" ry="4" fill="#EAF4FB" opacity=".9"/><circle cx="20" cy="23" r="10" fill="#F6C93E"/><path d="M11 21h18M10.5 26h19" stroke="#2E2B29" stroke-width="2.4"/><circle cx="16.2" cy="18.5" r="1.3" fill="#1F1A17"/><circle cx="23.8" cy="18.5" r="1.3" fill="#1F1A17"/><path d="M17 14q-1-4-3-5M23 14q1-4 3-5" stroke="#2E2B29" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Ladybug",
        r##"<circle cx="20" cy="22" r="11" fill="#E2483A"/><path d="M20 11v22" stroke="#2E2B29" stroke-width="1.2"/><g fill="#2E2B29"><circle cx="15" cy="20" r="1.8"/><circle cx="25" cy="20" r="1.8"/><circle cx="14.5" cy="27" r="1.6"/><circle cx="25.5" cy="27" r="1.6"/></g><path d="M12 15q8-6 16 0-8-4-16 0z" fill="#2E2B29"/><circle cx="17" cy="14" r=".9" fill="#FFF"/><circle cx="23" cy="14" r=".9" fill="#FFF"/>"##,
    ),
    (
        "Snail",
        r##"<circle cx="23" cy="22" r="9" fill="#D99A5B"/><path d="M23 22m-6 0a6 6 0 1 0 6-6a4 4 0 1 1-4 4a2 2 0 1 0 2-2" stroke="#A86A33" stroke-width="1.4" fill="none"/><path d="M5 32q4-3 10-3h14q-2 4-24 3z" fill="#B9C98E"/><path d="M8 29q-1-8 2-12M12 29q1-8 4-11" stroke="#B9C98E" stroke-width="2" fill="none" stroke-linecap="round"/><circle cx="10" cy="17" r="1.4" fill="#1F1A17"/><circle cx="16" cy="18" r="1.4" fill="#1F1A17"/>"##,
    ),
    (
        "Shark",
        r##"<path d="M20 10l4-6 2 7z" fill="#6E8EA8"/><ellipse cx="20" cy="22" rx="12" ry="10" fill="#7F9FB9"/><path d="M9 25q11 8 22 0-3 7-11 7S12 32 9 25z" fill="#EAF0F5"/><path d="M12 25.5l2 2 2-2 2 2 2-2 2 2 2-2 2 2 2-2" stroke="#FFF" stroke-width="1" fill="none"/><circle cx="15.4" cy="19" r="1.3" fill="#1F1A17"/><circle cx="24.6" cy="19" r="1.3" fill="#1F1A17"/>"##,
    ),
    (
        "Dolphin",
        r##"<path d="M19 9l4-5 1 6z" fill="#7F8FA0"/><circle cx="20" cy="21" r="10" fill="#93A3B4"/><ellipse cx="20" cy="28" rx="6" ry="4.6" fill="#93A3B4"/><ellipse cx="20" cy="29" rx="4.6" ry="3.2" fill="#E3E9EF"/><path d="M15.5 29q4.5 2.4 9 0" stroke="#5E6E80" stroke-width="1" fill="none" stroke-linecap="round"/><circle cx="15.6" cy="20" r="1.3" fill="#1F1A17"/><circle cx="24.4" cy="20" r="1.3" fill="#1F1A17"/>"##,
    ),
    (
        "Unicorn",
        r##"<path d="M20 2l-2 10h4z" fill="#F4C84A"/><path d="M12 15l1-6 4 4zM28 15l-1-6-4 4z" fill="#F7F3EE"/><path d="M11 14q-2 8 2 14M13 12q1 5 5 6" stroke="#C9A6E6" stroke-width="3" fill="none" stroke-linecap="round"/><ellipse cx="20" cy="22" rx="8.5" ry="10" fill="#FBF8F3"/><ellipse cx="20" cy="28" rx="5" ry="3.6" fill="#F4D8E0"/><circle cx="16.4" cy="21" r="1.2" fill="#1F1A17"/><circle cx="23.6" cy="21" r="1.2" fill="#1F1A17"/><circle cx="18" cy="28.5" r=".7" fill="#C28A9A"/><circle cx="22" cy="28.5" r=".7" fill="#C28A9A"/>"##,
    ),
    (
        "Dragon",
        r##"<path d="M13 13l-3-7 6 5zM27 13l3-7-6 5z" fill="#4E9E6E"/><circle cx="20" cy="22" r="10.5" fill="#5DB57E"/><path d="M17 12l1.5-3 1.5 3 1.5-3 1.5 3" fill="#F4C84A"/><ellipse cx="20" cy="27" rx="6.5" ry="4.4" fill="#A9DDB9"/><ellipse cx="16" cy="21" rx="1.3" ry="1.9" fill="#1F1A17"/><ellipse cx="24" cy="21" rx="1.3" ry="1.9" fill="#1F1A17"/><circle cx="18" cy="26.5" r=".8" fill="#3E7E56"/><circle cx="22" cy="26.5" r=".8" fill="#3E7E56"/>"##,
    ),
    (
        "Sloth",
        r##"<circle cx="20" cy="22" r="10.5" fill="#B39A82"/><ellipse cx="20" cy="23" rx="8.5" ry="7" fill="#E9DCCB"/><ellipse cx="15.5" cy="22" rx="3.4" ry="2.2" fill="#6A5848" transform="rotate(15 15.5 22)"/><ellipse cx="24.5" cy="22" rx="3.4" ry="2.2" fill="#6A5848" transform="rotate(-15 24.5 22)"/><circle cx="15.6" cy="22" r="1.1" fill="#FFF"/><circle cx="24.4" cy="22" r="1.1" fill="#FFF"/><ellipse cx="20" cy="25.5" rx="1.6" ry="1.1" fill="#3A2E25"/><path d="M18 28q2 1.2 4 0" stroke="#3A2E25" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Alpaca",
        r##"<ellipse cx="13" cy="10" rx="2" ry="4" fill="#EFE6DA" transform="rotate(-15 13 10)"/><ellipse cx="27" cy="10" rx="2" ry="4" fill="#EFE6DA" transform="rotate(15 27 10)"/><g fill="#F7F1E8"><circle cx="15" cy="11" r="4"/><circle cx="20" cy="9" r="4.5"/><circle cx="25" cy="11" r="4"/></g><ellipse cx="20" cy="23" rx="7.5" ry="10" fill="#F2E9DD"/><ellipse cx="20" cy="29" rx="4.6" ry="3.6" fill="#E3D3C0"/><circle cx="16.6" cy="21.5" r="1.3" fill="#1F1A17"/><circle cx="23.4" cy="21.5" r="1.3" fill="#1F1A17"/><path d="M18.6 29.5q1.4 1 2.8 0" stroke="#6A5848" stroke-width=".9" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Goat",
        r##"<path d="M14 12q-4-5-1-9M26 12q4-5 1-9" stroke="#B9A88F" stroke-width="2" fill="none" stroke-linecap="round"/><ellipse cx="9.5" cy="18" rx="3.6" ry="1.8" fill="#EDE7DD" transform="rotate(-20 9.5 18)"/><ellipse cx="30.5" cy="18" rx="3.6" ry="1.8" fill="#EDE7DD" transform="rotate(20 30.5 18)"/><ellipse cx="20" cy="21" rx="8" ry="10" fill="#F4EFE7"/><path d="M17 30q3 7 6 0z" fill="#E3DACC"/><circle cx="16.4" cy="20" r="1.2" fill="#1F1A17"/><circle cx="23.6" cy="20" r="1.2" fill="#1F1A17"/><circle cx="18.4" cy="27" r=".7" fill="#8A7A66"/><circle cx="21.6" cy="27" r=".7" fill="#8A7A66"/>"##,
    ),
    (
        "Horse",
        r##"<path d="M13 12l1-6 4 5zM27 12l-1-6-4 5z" fill="#8A5A36"/><path d="M17 7q3-3 6 0l-1 6h-4z" fill="#3A2A22"/><ellipse cx="20" cy="22" rx="8" ry="11" fill="#9C663E"/><ellipse cx="20" cy="29" rx="5.4" ry="4" fill="#C49572"/><circle cx="16.2" cy="19" r="1.2" fill="#1F1A17"/><circle cx="23.8" cy="19" r="1.2" fill="#1F1A17"/><circle cx="18" cy="29.5" r=".8" fill="#5A3A22"/><circle cx="22" cy="29.5" r=".8" fill="#5A3A22"/>"##,
    ),
    (
        "Donkey",
        r##"<ellipse cx="13.5" cy="8" rx="2.4" ry="6" fill="#8F8A86" transform="rotate(-12 13.5 8)"/><ellipse cx="26.5" cy="8" rx="2.4" ry="6" fill="#8F8A86" transform="rotate(12 26.5 8)"/><ellipse cx="20" cy="22" rx="8" ry="10.5" fill="#9E9893"/><ellipse cx="20" cy="29" rx="5.6" ry="4.2" fill="#E5E1DC"/><circle cx="16.2" cy="19.5" r="1.2" fill="#1F1A17"/><circle cx="23.8" cy="19.5" r="1.2" fill="#1F1A17"/><circle cx="18" cy="29" r=".8" fill="#5E5955"/><circle cx="22" cy="29" r=".8" fill="#5E5955"/>"##,
    ),
    (
        "Moose",
        r##"<path d="M12 12q-6-2-7-8 3 2 4 1 0 3 3 3M28 12q6-2 7-8-3 2-4 1 0 3-3 3" fill="#C9A06A"/><ellipse cx="20" cy="23" rx="8" ry="10" fill="#6E4A31"/><ellipse cx="20" cy="30" rx="6.5" ry="4.5" fill="#8A6045"/><circle cx="16.4" cy="20" r="1.2" fill="#1F1A17"/><circle cx="23.6" cy="20" r="1.2" fill="#1F1A17"/><circle cx="18" cy="30" r=".9" fill="#3A2A20"/><circle cx="22" cy="30" r=".9" fill="#3A2A20"/>"##,
    ),
    (
        "Walrus",
        r##"<circle cx="20" cy="22" r="10.5" fill="#B48A6E"/><ellipse cx="16.5" cy="26" rx="4" ry="3.2" fill="#D4AE92"/><ellipse cx="23.5" cy="26" rx="4" ry="3.2" fill="#D4AE92"/><path d="M17 28l-1 8M23 28l1 8" stroke="#FFFDF5" stroke-width="2.2" stroke-linecap="round"/><circle cx="15.8" cy="19.5" r="1.2" fill="#1F1A17"/><circle cx="24.2" cy="19.5" r="1.2" fill="#1F1A17"/><ellipse cx="20" cy="23.5" rx="1.4" ry="1" fill="#5A3A2A"/>"##,
    ),
    (
        "Polar Bear",
        r##"<circle cx="12" cy="14" r="3.4" fill="#ECEFF2"/><circle cx="28" cy="14" r="3.4" fill="#ECEFF2"/><circle cx="20" cy="22" r="10" fill="#F7F9FB"/><ellipse cx="20" cy="26" rx="4.8" ry="3.6" fill="#E3E7EB"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="24.8" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Red Panda",
        r##"<path d="M10.5 16l2-7 5 5zM29.5 16l-2-7-5 5z" fill="#C1502A"/><path d="M12 13l1-3 2.5 2.5zM28 13l-1-3-2.5 2.5z" fill="#FFF4EA"/><circle cx="20" cy="22" r="10" fill="#D35F30"/><ellipse cx="14.5" cy="22" rx="3" ry="2.4" fill="#FFF4EA"/><ellipse cx="25.5" cy="22" rx="3" ry="2.4" fill="#FFF4EA"/><ellipse cx="20" cy="27" rx="4.4" ry="3.2" fill="#FFF4EA"/><path d="M15 25q-1 3 1 5M25 25q1 3-1 5" stroke="#8A3218" stroke-width="1.4" fill="none"/><circle cx="15.6" cy="21" r="1.2" fill="#1F1A17"/><circle cx="24.4" cy="21" r="1.2" fill="#1F1A17"/><ellipse cx="20" cy="25.5" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Skunk",
        r##"<circle cx="12" cy="14" r="2.8" fill="#2E2B29"/><circle cx="28" cy="14" r="2.8" fill="#2E2B29"/><circle cx="20" cy="22" r="10" fill="#353130"/><path d="M20 12c-1.5 3-1.5 6 0 9 1.5-3 1.5-6 0-9z" fill="#F7F5F0"/><ellipse cx="20" cy="27" rx="4.4" ry="3.2" fill="#4A4542"/><circle cx="15.8" cy="21" r="1.2" fill="#FFF"/><circle cx="24.2" cy="21" r="1.2" fill="#FFF"/><ellipse cx="20" cy="25.5" rx="1.3" ry="1" fill="#F0B9C2"/>"##,
    ),
    (
        "Hamster",
        r##"<circle cx="12" cy="13" r="3" fill="#D99A5B"/><circle cx="28" cy="13" r="3" fill="#D99A5B"/><circle cx="12" cy="13" r="1.6" fill="#F0B9C2"/><circle cx="28" cy="13" r="1.6" fill="#F0B9C2"/><circle cx="20" cy="23" r="11" fill="#E5A866"/><ellipse cx="20" cy="27" rx="8" ry="6" fill="#FBF1E4"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="24.5" rx="1.2" ry="0.9" fill="#E28A9A"/><g stroke="#6B5E57" stroke-width=".7" stroke-linecap="round"><path d="M13 25.5h-4M13 27.0l-3.5 1M27 25.5h4M27 27.0l3.5 1"/></g>"##,
    ),
    (
        "Chipmunk",
        r##"<circle cx="12.5" cy="14" r="2.6" fill="#9C6A3E"/><circle cx="27.5" cy="14" r="2.6" fill="#9C6A3E"/><circle cx="20" cy="22" r="10" fill="#B07A48"/><path d="M14 18l2 2M26 18l-2 2" stroke="#3A2A22" stroke-width="1.6" stroke-linecap="round"/><path d="M20 12v6" stroke="#3A2A22" stroke-width="1.6" stroke-linecap="round"/><ellipse cx="20" cy="26.5" rx="6" ry="4.4" fill="#F4E2CC"/><circle cx="15.8" cy="21" r="1.3" fill="#1F1A17"/><circle cx="24.2" cy="21" r="1.3" fill="#1F1A17"/><ellipse cx="20" cy="25" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Mole",
        r##"<circle cx="20" cy="23" r="11" fill="#4A4246"/><ellipse cx="20" cy="27" rx="4" ry="3" fill="#F0A6B6"/><circle cx="20" cy="26" r="1.4" fill="#C9677D"/><path d="M15 21h2M23 21h2" stroke="#1A1416" stroke-width="1.4" stroke-linecap="round"/><path d="M10 33l3-3M30 33l-3-3" stroke="#F0A6B6" stroke-width="2.2" stroke-linecap="round"/>"##,
    ),
    (
        "Crocodile",
        r##"<rect x="9" y="16" width="22" height="17" rx="7" fill="#5E9B5A"/><circle cx="15" cy="15" r="3.2" fill="#5E9B5A"/><circle cx="25" cy="15" r="3.2" fill="#5E9B5A"/><circle cx="15" cy="14.5" r="1.8" fill="#FFF"/><circle cx="25" cy="14.5" r="1.8" fill="#FFF"/><circle cx="15" cy="14.8" r=".9" fill="#1F1A17"/><circle cx="25" cy="14.8" r=".9" fill="#1F1A17"/><path d="M11 27h18" stroke="#3E6E3B" stroke-width="1.2"/><path d="M13 27l1 2 1-2 1 2 1-2 1 2 1-2 1 2 1-2 1 2 1-2 1 2 1-2" stroke="#FFF" stroke-width=".8" fill="none"/><circle cx="17" cy="21" r=".7" fill="#3E6E3B"/><circle cx="23" cy="21" r=".7" fill="#3E6E3B"/>"##,
    ),
    (
        "Lizard",
        r##"<circle cx="20" cy="22" r="10" fill="#8CC152"/><g fill="#6E9E3E"><circle cx="14" cy="16" r="1.2"/><circle cx="26" cy="15" r="1"/><circle cx="20" cy="13" r="1.1"/></g><circle cx="15" cy="21" r="2.4" fill="#FFF"/><circle cx="25" cy="21" r="2.4" fill="#FFF"/><circle cx="15" cy="21" r="1.2" fill="#1F1A17"/><circle cx="25" cy="21" r="1.2" fill="#1F1A17"/><path d="M15 27q5 3 10 0" stroke="#3E6E2B" stroke-width="1.1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Snake",
        r##"<path d="M20 40q-8-5 0-10t0-12" stroke="#7FBF5A" stroke-width="7" fill="none" stroke-linecap="round"/><circle cx="20" cy="16" r="8" fill="#8CCB66"/><circle cx="16.8" cy="15" r="1.2" fill="#1F1A17"/><circle cx="23.2" cy="15" r="1.2" fill="#1F1A17"/><path d="M20 22v3l-1.4 1.4M20 25l1.4 1.4" stroke="#E2483A" stroke-width=".9" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Chameleon",
        r##"<path d="M12 18q8-10 18-4" fill="#2FA88C"/><circle cx="20" cy="22" r="10.5" fill="#3DBA9C"/><circle cx="24" cy="20" r="4" fill="#5FD0B5"/><circle cx="24" cy="20" r="2" fill="#1F1A17"/><path d="M12 27q8 3 14-1" stroke="#1E7A64" stroke-width="1.2" fill="none" stroke-linecap="round"/><path d="M28 30q4 4 1 6" stroke="#E86B9A" stroke-width="1.2" fill="none"/>"##,
    ),
    (
        "Toucan",
        r##"<circle cx="20" cy="22" r="10.5" fill="#2E2B29"/><ellipse cx="17" cy="23" rx="5" ry="6" fill="#F7F5F0"/><circle cx="17" cy="19" r="2.4" fill="#4FA6E0"/><circle cx="17" cy="19" r="1.1" fill="#1F1A17"/><path d="M21 18q11-1 13 6-5 2-13 2z" fill="#F4A132"/><path d="M21 22q8 0 13 2" stroke="#E2483A" stroke-width="1.4" fill="none"/>"##,
    ),
    (
        "Pelican",
        r##"<circle cx="20" cy="18" r="9.5" fill="#F7F5F0"/><circle cx="16.6" cy="16.5" r="1.1" fill="#1F1A17"/><circle cx="23.4" cy="16.5" r="1.1" fill="#1F1A17"/><path d="M17 20q3 0 6 0 6 4 4 13-4 2-8-1-4-6-2-12z" fill="#F4C84A"/><path d="M19 21q4 3 7 11" stroke="#D9A830" stroke-width="1" fill="none"/>"##,
    ),
    (
        "Eagle",
        r##"<path d="M10 16q10-8 20 0-10 2-20 0z" fill="#6E4A31"/><circle cx="20" cy="22" r="10" fill="#FAFAF7"/><path d="M12 18q4-3 7-1M28 18q-4-3-7-1" stroke="#8A6A50" stroke-width="1.2" fill="none"/><circle cx="16" cy="21" r="1.3" fill="#1F1A17"/><circle cx="24" cy="21" r="1.3" fill="#1F1A17"/><path d="M17 24q3-2 6 0 1 4-3 6-1-3-3-6z" fill="#F4B53A"/>"##,
    ),
    (
        "Hen",
        r##"<path d="M17 11q1-4 3-2 1-3 3 0 2-2 2 2z" fill="#E2483A"/><circle cx="20" cy="22" r="10.5" fill="#F7F2EA"/><circle cx="16" cy="20" r="1.3" fill="#1F1A17"/><circle cx="24" cy="20" r="1.3" fill="#1F1A17"/><path d="M18 23.5h4L20 26.5z" fill="#F0A33A"/><path d="M19 27q1 3 2 0z" fill="#E2483A"/>"##,
    ),
    (
        "Rooster",
        r##"<path d="M15 12q0-5 3-4 1-4 4-1 3-3 4 1 3 0 1 4z" fill="#E2483A"/><circle cx="20" cy="22" r="10.5" fill="#C9683A"/><path d="M12 24q8 8 16 0-2 8-8 8t-8-8z" fill="#3B7A5A"/><circle cx="16" cy="20" r="1.3" fill="#1F1A17"/><circle cx="24" cy="20" r="1.3" fill="#1F1A17"/><path d="M18 23.5h4L20 26.5z" fill="#F0A33A"/><path d="M19 27q1 3 2 0z" fill="#E2483A"/>"##,
    ),
    (
        "Goose",
        r##"<path d="M20 40q-4-8 0-16" stroke="#E9ECEF" stroke-width="7" fill="none" stroke-linecap="round"/><circle cx="20" cy="17" r="8.5" fill="#F4F6F8"/><circle cx="16.6" cy="15.5" r="1.2" fill="#1F1A17"/><circle cx="23.4" cy="15.5" r="1.2" fill="#1F1A17"/><path d="M16 20q4-2 8 0-1 3-4 3t-4-3z" fill="#EE8F2E"/>"##,
    ),
    (
        "Pigeon",
        r##"<circle cx="20" cy="22" r="10.5" fill="#9AA0B4"/><path d="M11 27q9 6 18 0-2 6-9 6t-9-6z" fill="#7FB3A0"/><circle cx="16" cy="20" r="2" fill="#F4A132"/><circle cx="24" cy="20" r="2" fill="#F4A132"/><circle cx="16" cy="20" r="1" fill="#1F1A17"/><circle cx="24" cy="20" r="1" fill="#1F1A17"/><path d="M18.5 23h3L20 26z" fill="#4A4E5A"/>"##,
    ),
    (
        "Hummingbird",
        r##"<path d="M8 16q6-6 10 2M32 16q-6-6-10 2" fill="#7FD0C2" opacity=".9"/><circle cx="20" cy="22" r="9" fill="#3BA57A"/><path d="M13 26q7 5 14 0-2 5-7 5t-7-5z" fill="#E2483A"/><circle cx="16.6" cy="21" r="1.2" fill="#1F1A17"/><circle cx="23.4" cy="21" r="1.2" fill="#1F1A17"/><path d="M20 24l14 3-14-1z" fill="#3A3633"/>"##,
    ),
    (
        "Kiwi",
        r##"<circle cx="20" cy="21" r="11" fill="#8A6A4A"/><path d="M13 15l1 2M17 13l.5 2M23 13l-.5 2M27 15l-1 2M12 22l2 .5M28 22l-2 .5" stroke="#6E5236" stroke-width="1" stroke-linecap="round"/><circle cx="16" cy="19.5" r="1.3" fill="#1F1A17"/><circle cx="24" cy="19.5" r="1.3" fill="#1F1A17"/><path d="M19 23q1 .5 2 0l3 13q-.8.8-1.6 0z" fill="#E7C9A0"/>"##,
    ),
    (
        "Ostrich",
        r##"<path d="M20 40q-2-10 0-18" stroke="#F0C9B0" stroke-width="5" fill="none" stroke-linecap="round"/><g fill="#5E5651"><circle cx="16" cy="11" r="2.4"/><circle cx="20" cy="9" r="2.6"/><circle cx="24" cy="11" r="2.4"/></g><circle cx="20" cy="16" r="7.5" fill="#F4D2BE"/><circle cx="17" cy="15" r="2.2" fill="#FFF"/><circle cx="23" cy="15" r="2.2" fill="#FFF"/><circle cx="17" cy="15" r="1.1" fill="#1F1A17"/><circle cx="23" cy="15" r="1.1" fill="#1F1A17"/><path d="M17.5 19q2.5 2 5 0-2.5 2-5 0z" fill="#E8A13A" stroke="#E8A13A" stroke-width="1"/>"##,
    ),
    (
        "Jellyfish",
        r##"<path d="M12 24q0 6-2 10M16 25q0 6 1 10M20 25v11M24 25q0 6-1 10M28 24q0 6 2 10" stroke="#C9A6E6" stroke-width="1.6" fill="none" stroke-linecap="round"/><path d="M9 25q0-14 11-14t11 14q-5-2-11-2t-11 2z" fill="#B48CE0"/><circle cx="16.4" cy="19" r="1.3" fill="#1F1A17"/><circle cx="23.6" cy="19" r="1.3" fill="#1F1A17"/><path d="M18 22q2 1.4 4 0" stroke="#1F1A17" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Starfish",
        r##"<path d="M20 5l4 10 10 1-8 7 3 11-9-6-9 6 3-11-8-7 10-1z" fill="#F4A06A" stroke="#E07A3A" stroke-width="1" stroke-linejoin="round"/><circle cx="17.4" cy="19" r="1.1" fill="#1F1A17"/><circle cx="22.6" cy="19" r="1.1" fill="#1F1A17"/><path d="M18.5 22q1.5 1.2 3 0" stroke="#1F1A17" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Clownfish",
        r##"<path d="M30 22l7-6v12z" fill="#F07A2E"/><ellipse cx="19" cy="22" rx="12" ry="9" fill="#F28A3A"/><path d="M13 13.5q2 8.5 0 17M22 13q2 9 0 18" stroke="#FFF" stroke-width="2.6" fill="none"/><path d="M13 13.5q2 8.5 0 17M22 13q2 9 0 18" stroke="#2E2B29" stroke-width=".6" fill="none"/><circle cx="10.5" cy="20" r="1.6" fill="#1F1A17"/><path d="M8 25q2 1 3.5 0" stroke="#1F1A17" stroke-width=".9" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Pufferfish",
        r##"<g stroke="#D9A830" stroke-width="1.2" stroke-linecap="round"><path d="M20 8v-3M12 11l-2-2M28 11l2-2M8 20H5M32 20h3M10 29l-2 2M30 29l2 2M20 33v3"/></g><circle cx="20" cy="20" r="12" fill="#F4C84A"/><ellipse cx="20" cy="25" rx="8" ry="5" fill="#FBEBB8"/><circle cx="15.6" cy="18" r="1.6" fill="#1F1A17"/><circle cx="24.4" cy="18" r="1.6" fill="#1F1A17"/><circle cx="20" cy="25" r="1.4" fill="#E28A6A"/>"##,
    ),
    (
        "Seahorse",
        r##"<path d="M22 38q-6-2-4-8t6-8q0-4-4-6l-2-4q6-1 9 3 2 4 0 9-4 4-3 8 1 3-2 6z" fill="#F4A06A"/><path d="M16 10l-5 1 5 2z" fill="#F4A06A"/><circle cx="20" cy="12" r="1.3" fill="#1F1A17"/><path d="M24 14l2-3M26 18l3-2" stroke="#E07A3A" stroke-width="1.2" stroke-linecap="round"/>"##,
    ),
    (
        "Butterfly",
        r##"<ellipse cx="12" cy="15" rx="7" ry="6.5" fill="#F48AB0"/><ellipse cx="28" cy="15" rx="7" ry="6.5" fill="#F48AB0"/><ellipse cx="13" cy="27" rx="5" ry="5" fill="#9C8AE6"/><ellipse cx="27" cy="27" rx="5" ry="5" fill="#9C8AE6"/><circle cx="12" cy="15" r="2.2" fill="#FFF" opacity=".8"/><circle cx="28" cy="15" r="2.2" fill="#FFF" opacity=".8"/><rect x="18.4" y="11" width="3.2" height="20" rx="1.6" fill="#3A3633"/><path d="M19 11q-2-4-4-5M21 11q2-4 4-5" stroke="#3A3633" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Ant",
        r##"<path d="M14 14q-3-5-6-5M26 14q3-5 6-5" stroke="#3A2A22" stroke-width="1.2" fill="none" stroke-linecap="round"/><path d="M11 26l-5 3M11 30l-4 5M29 26l5 3M29 30l4 5" stroke="#3A2A22" stroke-width="1.4" stroke-linecap="round"/><circle cx="20" cy="19" r="8.5" fill="#8A3A22"/><ellipse cx="20" cy="31" rx="7" ry="5" fill="#7A3220"/><circle cx="16.8" cy="18.5" r="1.3" fill="#FFF"/><circle cx="23.2" cy="18.5" r="1.3" fill="#FFF"/><circle cx="16.8" cy="18.8" r=".6" fill="#1F1A17"/><circle cx="23.2" cy="18.8" r=".6" fill="#1F1A17"/>"##,
    ),
    (
        "Spider",
        r##"<path d="M12 18l-6-4M11 22H4M12 26l-6 4M28 18l6-4M29 22h7M28 26l6 4" stroke="#3A3633" stroke-width="1.6" stroke-linecap="round"/><circle cx="20" cy="22" r="9" fill="#4A4542"/><circle cx="16.5" cy="20" r="2.4" fill="#FFF"/><circle cx="23.5" cy="20" r="2.4" fill="#FFF"/><circle cx="16.5" cy="20.3" r="1.2" fill="#1F1A17"/><circle cx="23.5" cy="20.3" r="1.2" fill="#1F1A17"/><path d="M17.5 25.5q2.5 2 5 0" stroke="#F0B9C2" stroke-width="1.1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Caterpillar",
        r##"<g fill="#8CCB66"><circle cx="9" cy="28" r="5"/><circle cx="15" cy="30" r="5"/><circle cx="21" cy="30" r="5"/></g><circle cx="20" cy="20" r="8" fill="#9CD676"/><path d="M16 13l-2-5M24 13l2-5" stroke="#3A3633" stroke-width="1.2" stroke-linecap="round"/><circle cx="14" cy="8" r="1.2" fill="#E2483A"/><circle cx="26" cy="8" r="1.2" fill="#E2483A"/><circle cx="16.8" cy="19.5" r="1.2" fill="#1F1A17"/><circle cx="23.2" cy="19.5" r="1.2" fill="#1F1A17"/><path d="M18 23q2 1.4 4 0" stroke="#1F1A17" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Gorilla",
        r##"<circle cx="10.5" cy="21" r="3" fill="#3A3633"/><circle cx="29.5" cy="21" r="3" fill="#3A3633"/><circle cx="20" cy="22" r="10.5" fill="#3A3633"/><path d="M13 22c0-4 3-6 7-6s7 2 7 6c0 5-3 9-7 9s-7-4-7-9z" fill="#6E6762"/><path d="M14 18h12" stroke="#2A2624" stroke-width="2.2" stroke-linecap="round"/><circle cx="16.6" cy="20.5" r="1.2" fill="#1F1A17"/><circle cx="23.4" cy="20.5" r="1.2" fill="#1F1A17"/><ellipse cx="18.5" cy="25" rx="1" ry=".7" fill="#2A2624"/><ellipse cx="21.5" cy="25" rx="1" ry=".7" fill="#2A2624"/><path d="M17.5 28q2.5 1.4 5 0" stroke="#2A2624" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Orangutan",
        r##"<path d="M9 14q2-6 11-6t11 6" stroke="#C25A22" stroke-width="3" fill="none" stroke-linecap="round"/><circle cx="20" cy="22" r="10.5" fill="#C9632A"/><ellipse cx="20" cy="24" rx="7.5" ry="6.5" fill="#E8A57A"/><circle cx="16.6" cy="21" r="1.2" fill="#1F1A17"/><circle cx="23.4" cy="21" r="1.2" fill="#1F1A17"/><path d="M17 27.5q3 2 6 0" stroke="#5A2A12" stroke-width="1.1" fill="none" stroke-linecap="round"/><circle cx="19" cy="24.5" r=".6" fill="#5A2A12"/><circle cx="21" cy="24.5" r=".6" fill="#5A2A12"/>"##,
    ),
    (
        "Panther",
        r##"<path d="M10.5 16l1.5-8 6 5zM29.5 16l-1.5-8-6 5z" fill="#2B2A30"/><circle cx="20" cy="22" r="10" fill="#33323A"/><circle cx="16" cy="21" r="2.2" fill="#F4C84A"/><circle cx="24" cy="21" r="2.2" fill="#F4C84A"/><ellipse cx="16" cy="21" rx=".6" ry="1.6" fill="#1F1A17"/><ellipse cx="24" cy="21" rx=".6" ry="1.6" fill="#1F1A17"/><ellipse cx="20" cy="25.5" rx="1.4" ry="1" fill="#6E6A78"/><g stroke="#7E7A88" stroke-width=".7" stroke-linecap="round"><path d="M13 26.5h-4M13 28.0l-3.5 1M27 26.5h4M27 28.0l3.5 1"/></g>"##,
    ),
    (
        "Leopard",
        r##"<circle cx="12" cy="14" r="3" fill="#E8B456"/><circle cx="28" cy="14" r="3" fill="#E8B456"/><circle cx="20" cy="22" r="10" fill="#F0C266"/><g fill="#7A4E22"><circle cx="14" cy="17" r="1.1"/><circle cx="26" cy="16" r="1"/><circle cx="20" cy="14" r="1.1"/><circle cx="12.5" cy="23" r="1"/><circle cx="27.5" cy="23" r="1.1"/></g><ellipse cx="20" cy="26.5" rx="4.6" ry="3.6" fill="#FBEBD0"/><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="25" rx="1.8" ry="1.2" fill="#7A4E22"/>"##,
    ),
    (
        "Cheetah",
        r##"<circle cx="12" cy="14" r="2.8" fill="#E8B456"/><circle cx="28" cy="14" r="2.8" fill="#E8B456"/><circle cx="20" cy="22" r="10" fill="#F2C870"/><ellipse cx="20" cy="26.5" rx="4.6" ry="3.6" fill="#FBEBD0"/><path d="M15.5 21.5q-.5 3 1.5 5.5M24.5 21.5q.5 3-1.5 5.5" stroke="#3A2A22" stroke-width="1.2" fill="none" stroke-linecap="round"/><g fill="#3A2A22"><circle cx="15" cy="16" r=".9"/><circle cx="25" cy="16" r=".9"/><circle cx="20" cy="13.5" r=".9"/><circle cx="12" cy="20" r=".8"/><circle cx="28" cy="20" r=".8"/></g><circle cx="16" cy="20.5" r="1.4" fill="#1F1A17"/><circle cx="24" cy="20.5" r="1.4" fill="#1F1A17"/><ellipse cx="20" cy="25" rx="1.8" ry="1.2" fill="#1F1A17"/>"##,
    ),
    (
        "Meerkat",
        r##"<ellipse cx="20" cy="22" rx="7" ry="11" fill="#D9B88A"/><ellipse cx="16.5" cy="19.5" rx="2.6" ry="2" fill="#5A4430"/><ellipse cx="23.5" cy="19.5" rx="2.6" ry="2" fill="#5A4430"/><circle cx="16.5" cy="19.5" r="1.1" fill="#FFF"/><circle cx="23.5" cy="19.5" r="1.1" fill="#FFF"/><circle cx="16.5" cy="19.5" r=".5" fill="#1F1A17"/><circle cx="23.5" cy="19.5" r=".5" fill="#1F1A17"/><ellipse cx="20" cy="25" rx="1.3" ry="1" fill="#3A2A20"/><circle cx="12.5" cy="16" r="1.4" fill="#5A4430"/><circle cx="27.5" cy="16" r="1.4" fill="#5A4430"/>"##,
    ),
    (
        "Platypus",
        r##"<circle cx="20" cy="20" r="10.5" fill="#8A6A4A"/><circle cx="16" cy="18.5" r="1.3" fill="#1F1A17"/><circle cx="24" cy="18.5" r="1.3" fill="#1F1A17"/><path d="M12 24q8-3 16 0 2 6-8 7-10-1-8-7z" fill="#3A3633"/><circle cx="18" cy="25.5" r=".6" fill="#6E6A66"/><circle cx="22" cy="25.5" r=".6" fill="#6E6A66"/>"##,
    ),
    (
        "Wombat",
        r##"<circle cx="12" cy="13" r="2.6" fill="#7A6450"/><circle cx="28" cy="13" r="2.6" fill="#7A6450"/><circle cx="20" cy="23" r="11" fill="#8A7460"/><ellipse cx="20" cy="26" rx="5" ry="4" fill="#5A4636"/><circle cx="15.6" cy="20.5" r="1.1" fill="#1F1A17"/><circle cx="24.4" cy="20.5" r="1.1" fill="#1F1A17"/><path d="M17 28q3 1.4 6 0" stroke="#3A2E24" stroke-width="1" fill="none" stroke-linecap="round"/>"##,
    ),
    (
        "Narwhal",
        r##"<path d="M22 13l9-11" stroke="#F4EBD6" stroke-width="2" stroke-linecap="round"/><ellipse cx="20" cy="23" rx="12" ry="10" fill="#8FA9C6"/><g fill="#6E89A8"><circle cx="13" cy="18" r="1"/><circle cx="27" cy="18" r="1.2"/><circle cx="20" cy="15" r="1"/></g><path d="M9 25q11 8 22 0-3 7-11 7S12 32 9 25z" fill="#E4ECF4"/><circle cx="15.6" cy="21" r="1.3" fill="#1F1A17"/><circle cx="24.4" cy="21" r="1.3" fill="#1F1A17"/>"##,
    ),
    (
        "Axolotl",
        r##"<path d="M9 16l-4-2M9 20H4M9 24l-4 2M31 16l4-2M31 20h5M31 24l4 2" stroke="#E86B9A" stroke-width="2.2" stroke-linecap="round"/><circle cx="20" cy="21" r="11" fill="#F7B6CC"/><circle cx="15" cy="20" r="1.4" fill="#1F1A17"/><circle cx="25" cy="20" r="1.4" fill="#1F1A17"/><path d="M15 25q5 4 10 0" stroke="#B0436C" stroke-width="1.2" fill="none" stroke-linecap="round"/><circle cx="12.5" cy="24" r="1.4" fill="#F28AAE" opacity=".8"/><circle cx="27.5" cy="24" r="1.4" fill="#F28AAE" opacity=".8"/>"##,
    ),
    (
        "Yak",
        r##"<path d="M11 14q-6-2-6-7 3 3 6 3M29 14q6-2 6-7-3 3-6 3" stroke="#E7E2D8" stroke-width="2.4" fill="none" stroke-linecap="round"/><path d="M10 16q0-8 10-8t10 8v12q-10 6-20 0z" fill="#5A4636"/><path d="M13 12q7-4 14 0-7 2-14 0z" fill="#4A3828"/><ellipse cx="20" cy="28" rx="5" ry="4" fill="#3A2E24"/><circle cx="16.4" cy="21" r="1.2" fill="#FFF"/><circle cx="23.6" cy="21" r="1.2" fill="#FFF"/><circle cx="18" cy="28.5" r=".7" fill="#8A7A66"/><circle cx="22" cy="28.5" r=".7" fill="#8A7A66"/>"##,
    ),
];

pub const ADJECTIVES: [(&str, &str); 32] = [
    ("Amber", "#F6C98A"),
    ("Quiet", "#DCE3EA"),
    ("Bright", "#FFE79A"),
    ("Calm", "#CFE6E6"),
    ("Clever", "#E3DAF3"),
    ("Gentle", "#F6DCE4"),
    ("Golden", "#F3D27A"),
    ("Happy", "#FFD9A8"),
    ("Lucky", "#CDEBC8"),
    ("Merry", "#F8CBC0"),
    ("Misty", "#D9DFE8"),
    ("Noble", "#D6D2EE"),
    ("Polar", "#D8EEF7"),
    ("Rapid", "#FFC9B0"),
    ("Silver", "#E2E4E8"),
    ("Sunny", "#FFE58F"),
    ("Swift", "#CFE0FA"),
    ("Tidy", "#E4EDD6"),
    ("Brave", "#F6C3B6"),
    ("Cosy", "#F1DCC4"),
    ("Crisp", "#D2F0E7"),
    ("Dapper", "#D8DCEC"),
    ("Eager", "#FFD7A0"),
    ("Fuzzy", "#EBDDD0"),
    ("Jolly", "#FFCFD8"),
    ("Keen", "#D5ECF0"),
    ("Lively", "#D9F2C6"),
    ("Mellow", "#EFE3C8"),
    ("Nimble", "#DCEBD5"),
    ("Plucky", "#FBD2C3"),
    ("Rosy", "#F9CCD6"),
    ("Witty", "#E2D9F6"),
];
