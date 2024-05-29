var searchIndex = new Map(JSON.parse('[\
["imxrt_usbd",{"doc":"A USB driver for i.MX RT processors","t":"FFFPPSKGNNNNNNNNNNNNNNNNNNNNNNCNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNMMNNNFPPGGPPNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN","n":["BusAdapter","EndpointMemory","EndpointState","High","LowFull","MAX_ENDPOINTS","Peripherals","Speed","alloc_ep","borrow","borrow","borrow","borrow","borrow_mut","borrow_mut","borrow_mut","borrow_mut","clone","configure","default","default","default","enable","enable_zlt","eq","fmt","from","from","from","from","gpt","gpt_mut","into","into","into","into","is_stalled","max_endpoints","new","new","new","poll","read","reset","resume","set_device_address","set_interrupts","set_stalled","suspend","try_from","try_from","try_from","try_from","try_into","try_into","try_into","try_into","type_id","type_id","type_id","type_id","usb","usbphy","with_speed","without_critical_sections","write","Gpt","Gpt0","Gpt1","Instance","Mode","OneShot","Repeat","borrow","borrow","borrow","borrow_mut","borrow_mut","borrow_mut","clear_elapsed","clone","clone","eq","eq","fmt","fmt","from","from","from","instance","into","into","into","is_elapsed","is_interrupt_enabled","is_running","load","mode","reset","run","set_interrupt_enabled","set_load","set_mode","stop","try_from","try_from","try_from","try_into","try_into","try_into","type_id","type_id","type_id"],"q":[[0,"imxrt_usbd"],[66,"imxrt_usbd::gpt"],[113,"usb_device"],[114,"usb_device::endpoint"],[115,"core::option"],[116,"usb_device::endpoint"],[117,"core::fmt"],[118,"usb_device::bus"],[119,"core::result"],[120,"core::any"]],"d":["A full- and high-speed <code>UsbBus</code> implementation","Memory for endpoint I/O.","Driver state associated with endpoints.","High speed.","Throttle to low / full speeds.","The maximum supported number of endpoints.","A type that owns all USB register blocks","USB low / full / high speed setting.","","","","","","","","","","","Apply device configurations, and perform other …","","","","","Enable zero-length termination (ZLT) for the given endpoint","","","Returns the argument unchanged.","Returns the argument unchanged.","Returns the argument unchanged.","Returns the argument unchanged.","USB general purpose timers.","Acquire one of the GPT timer instances.","Calls <code>U::from(self)</code>.","Calls <code>U::from(self)</code>.","Calls <code>U::from(self)</code>.","Calls <code>U::from(self)</code>.","","Allocate space for the maximum number of endpoints.","Allocate endpoint memory.","Create a high-speed USB bus adapter","Allocate state for <code>COUNT</code> endpoints.","","","","","","Enable (<code>true</code>) or disable (<code>false</code>) interrupts for this USB …","","","","","","","","","","","","","","","Returns the pointer to the USB register block.","Returns the pointer to the USBPHY register block.","Create a USB bus adapter with the given speed","Create a USB bus adapter that never takes a critical …","","General purpose timer (GPT).","The GPT0 timer instance.","The GPT1 timer instance.","GPT instance identifiers.","GPT timer mode.","In one shot mode, the timer will count down to zero, …","In repeat mode, the timer will count down to zero, …","","","","","","","Clear the flag that indicates the timer has elapsed.","","","","","","","Returns the argument unchanged.","Returns the argument unchanged.","Returns the argument unchanged.","Returns the GPT instance identifier.","Calls <code>U::from(self)</code>.","Calls <code>U::from(self)</code>.","Calls <code>U::from(self)</code>.","Indicates if the timer has elapsed.","Indicates if interrupt generation is enabled.","Indicates if the timer is running (<code>true</code>) or stopped (<code>false</code>…","Returns the counter load value.","Returns the timer mode.","Reset the timer.","Run the GPT timer.","Enable or disable interrupt generation when the timer …","Set the counter load value.","Set the timer mode.","Stop the timer.","","","","","","","","",""],"i":[0,0,0,9,9,0,0,0,1,11,1,12,9,11,1,12,9,9,1,11,12,9,1,1,9,9,11,1,12,9,0,1,11,1,12,9,1,12,11,1,12,1,1,1,1,1,1,1,1,11,1,12,9,11,1,12,9,11,1,12,9,20,20,1,1,1,0,16,16,0,0,26,26,17,26,16,17,26,16,17,26,16,26,16,26,16,17,26,16,17,17,26,16,17,17,17,17,17,17,17,17,17,17,17,17,26,16,17,26,16,17,26,16],"f":"````````{{bd{h{f}}jln}{{A`{f}}}}{ce{}{}}0000000{AbAb}{bAd}{{}Af}{{}Ah}{{}Ab}3{{bf}Ad}{{AbAb}Aj}{{AbAl}An}{cc{}}000`{{bB`e}c{}{{Bf{Bb}{{Bd{c}}}}}}::::{{bf}Aj}78{{cAfAh}bBh}8{bBj}{{bf{Bl{n}}}{{A`{Bn}}}}<<{{bn}Ad}{{bAj}Ad}{{bfAj}Ad}?{c{{C`{e}}}{}{}}0000000{cCb{}}000{BhAd}0{{cAfAhAb}bBh}07```````{ce{}{}}00000{BbAd}{CdCd}{B`B`}{{CdCd}Aj}{{B`B`}Aj}{{CdAl}An}{{B`Al}An}{cc{}}00{BbB`}999{BbAj}00{BbCf}{BbCd};;{{BbAj}Ad}{{BbCf}Ad}{{BbCd}Ad}>{c{{C`{e}}}{}{}}00000{cCb{}}00","c":[],"p":[[5,"BusAdapter",0],[6,"UsbDirection",113],[5,"EndpointAddress",114],[6,"Option",115],[6,"EndpointType",114],[1,"u16"],[1,"u8"],[8,"Result",113],[6,"Speed",0],[1,"unit"],[5,"EndpointMemory",0],[5,"EndpointState",0],[1,"bool"],[5,"Formatter",116],[8,"Result",116],[6,"Instance",66],[5,"Gpt",66],[17,"Output"],[10,"FnOnce",117],[10,"Peripherals",0],[6,"PollResult",118],[1,"slice"],[1,"usize"],[6,"Result",119],[5,"TypeId",120],[6,"Mode",66],[1,"u32"]],"b":[]}]\
]'));
if (typeof exports !== 'undefined') exports.searchIndex = searchIndex;
else if (window.initSearch) window.initSearch(searchIndex);
