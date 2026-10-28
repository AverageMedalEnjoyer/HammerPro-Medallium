use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    TerminateJobObject,
};

pub(crate) struct Job {
    handle: HANDLE,
}

unsafe impl Send for Job {}

impl Job {
    pub(crate) fn for_child(child: &Child) -> Option<Job> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return None;
        }
        let job = Job { handle };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).ok()?;
        let ok = unsafe {
            SetInformationJobObject(job.handle, JobObjectExtendedLimitInformation, (&raw const info).cast(), size)
        };
        if ok == 0 {
            return None;
        }
        let process: HANDLE = child.as_raw_handle().cast();
        let ok = unsafe { AssignProcessToJobObject(job.handle, process) };
        (ok != 0).then_some(job)
    }

    pub(crate) fn terminate(&self) {
        unsafe {
            TerminateJobObject(self.handle, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
