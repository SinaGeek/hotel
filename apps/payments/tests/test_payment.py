from django.test import TestCase
from apps.payments.services.payment_service import PaymentService
from apps.hotels.models import Hotel
from apps.rooms.models import RoomType, Room
from apps.guests.models import Guest
from apps.reservations.models import Reservation
from apps.folios.models import Folio
from datetime import date
class PaymentTest(TestCase):
    def test_payment_created(self):
        h=Hotel.objects.create(name='H',code='h1'); rt=RoomType.objects.create(hotel_id=h.id,name='Std',base_rate=100)
        r=Room.objects.create(hotel_id=h.id,number='101',room_type=rt); g=Guest.objects.create(hotel_id=h.id,first_name='A',last_name='B')
        res=Reservation.objects.create(hotel_id=h.id,guest=g,room=r,check_in_date=date(2026,1,1),check_out_date=date(2026,1,2),source='WALK_IN')
        f=Folio.objects.create(hotel_id=h.id,reservation=res)
        p=PaymentService.create_payment(hotel_id=h.id,folio=f,method='CASH',amount=10)
        self.assertEqual(p.status,'PENDING')
