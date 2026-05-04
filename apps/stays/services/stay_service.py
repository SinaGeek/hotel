from django.db import transaction
from django.utils import timezone
from apps.stays.models import Stay
from apps.rooms.models import RoomStatus
from apps.housekeeping.models import HousekeepingTask

class StayService:
    @staticmethod
    @transaction.atomic
    def check_in(reservation):
        stay=Stay.objects.create(hotel_id=reservation.hotel_id,reservation=reservation,room=reservation.room,actual_check_in=timezone.now())
        reservation.room.status=RoomStatus.OCCUPIED; reservation.room.save(update_fields=['status'])
        return stay
    @staticmethod
    @transaction.atomic
    def check_out(stay):
        stay.actual_check_out=timezone.now(); stay.save(update_fields=['actual_check_out'])
        stay.room.status=RoomStatus.DIRTY; stay.room.save(update_fields=['status'])
        HousekeepingTask.objects.create(hotel_id=stay.hotel_id,room=stay.room,status='PENDING',priority=5)
