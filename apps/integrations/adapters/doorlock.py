from abc import ABC, abstractmethod
class DoorLockAdapter(ABC):
    @abstractmethod
    def issue_key(self, room, guest, check_out_time): ...
    @abstractmethod
    def revoke_key(self, room): ...
    @abstractmethod
    def reissue_key(self, room, guest): ...
class MockDoorLockAdapter(DoorLockAdapter):
    def issue_key(self, room, guest, check_out_time): return {'room':room.number,'status':'issued'}
    def revoke_key(self, room): return {'room':room.number,'status':'revoked'}
    def reissue_key(self, room, guest): return {'room':room.number,'status':'reissued'}
